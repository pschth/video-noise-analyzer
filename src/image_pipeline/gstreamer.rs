use std::sync::{atomic::AtomicBool, Arc, Mutex, MutexGuard};

use gst::{
    event::{FlushStart, FlushStop, Reconfigure},
    prelude::*,
    State,
};
use thiserror::Error;

use crate::image_pipeline::{device_caps::RawSourceCaps, frame_handler::FrameHandler};

#[derive(Debug, Error)]
pub enum GstError {
    #[error("Error: {0}")]
    Error(#[from] gst::glib::Error),

    #[error("Bool error: {0}")]
    BoolError(#[from] gst::glib::BoolError),

    #[error("No capabilities found.")]
    NoCapsFound,

    #[error("Element not found.")]
    ElementNotFound,

    #[error("Invalid value.")]
    InvalidValue,
}

pub struct ImagePipeline {
    pub pipeline: gst::Pipeline,
    pub frame_handler: Option<FrameHandler>,
    pub caps: Arc<Mutex<RawSourceCaps>>,
    shutdown_flag: AtomicBool,
}

impl Drop for ImagePipeline {
    fn drop(&mut self) {
        self.shutdown_flag.store(true, std::sync::atomic::Ordering::Relaxed);
        self.pipeline.set_state(gst::State::Null).unwrap();
    }
}

impl ImagePipeline {
    pub fn new() -> Result<Self, GstError> {
        gst::init()?;
        let caps_obj = RawSourceCaps::new()?;

        // create source element
        let source = gst::ElementFactory::make("v4l2src")
            .name("source")
            .build()
            .expect("Could not create v4l2src element. Make sure the v4l2 plugin is installed.");
        source.set_property("device", caps_obj.get_current_device_path());

        // set video capabilities to the first available capability
        let curr_res = caps_obj.get_current_resolution();
        let curr_framerate = caps_obj.get_current_framerate()?;
        println!("Selected resolution: {}, framerate: {}", &curr_res, &curr_framerate);
        let caps = gst::Caps::builder("video/x-raw")
            .field("width", curr_res.width)
            .field("height", curr_res.height)
            .field("framerate", curr_framerate)
            .build();

        let capsfilter = gst::ElementFactory::make("capsfilter")
            .name("filter")
            .build()
            .expect("Could not create capsfilter element.");
        capsfilter.set_property("caps", &caps);

        // add convert element to convert to RGB
        let convert = gst::ElementFactory::make("videoconvert")
            .name("convert")
            .build()
            .expect("Could not create videoconvert element.");

        // capsfilter to enforce RGB format in converted output
        let rgb_caps = gst::Caps::builder("video/x-raw").field("format", "RGB").build();

        let rgb_filter = gst::ElementFactory::make("capsfilter")
            .name("rgb_filter")
            .build()
            .expect("Could not create rgb capsfilter element.");
        rgb_filter.set_property("caps", &rgb_caps);

        // create appsink element to retrieve frames
        let sink = gst_app::AppSink::builder().build();
        // configure appsink to emit a signal when a new sample is ready
        sink.set_property("emit-signals", true);

        // build the pipeline
        let pipeline = gst::Pipeline::new();
        let sink: gst::Element = sink.upcast();
        pipeline
            .add_many([&source, &capsfilter, &convert, &rgb_filter, &sink])
            .expect("Failed to add elements to pipeline.");
        gst::Element::link_many([&source, &capsfilter, &convert, &rgb_filter, &sink])
            .expect("Failed to link elements in pipeline.");

        let img_pipeline = ImagePipeline {
            pipeline,
            caps: Arc::new(Mutex::new(caps_obj)),
            frame_handler: None,
            shutdown_flag: AtomicBool::new(false),
        };

        // set image pipeline to pause state if possible
        img_pipeline
            .set_state(State::Paused)
            .expect("Could not set pipeline into pause state.");

        Ok(img_pipeline)
    }

    /// sets the pipeline to the given state
    pub fn set_state(&self, state: gst::State) -> Result<gst::StateChangeSuccess, gst::StateChangeError> {
        set_current_state(&self.pipeline, state)
    }

    pub fn set_state_cb(
        pipeline: &gst::Pipeline,
        frame_handler: &FrameHandler,
        state: gst::State,
    ) -> Result<gst::StateChangeSuccess, gst::StateChangeError> {
        match pipeline.set_state(state) {
            Ok(r) => {
                match state {
                    State::Playing => {}
                    _ => frame_handler.display_pause_image(),
                };
                Ok(r)
            }
            Err(r) => {
                println!("Failed to set state");
                Err(r)
            }
        }
    }

    /// toggles between play and pause states of the pipeline
    pub fn toggle_play_pause(pipeline: &gst::Pipeline, frame_handler: &FrameHandler) -> State {
        match get_current_state(pipeline) {
            State::Playing => {
                if Self::set_state_cb(pipeline, frame_handler, State::Paused).is_err() {
                    eprintln!("Failed to pause video.");
                };
                return State::Paused;
            }
            _ => {
                if Self::set_state_cb(pipeline, frame_handler, State::Playing).is_err() {
                    eprintln!("Failed to play video.");
                };
                return State::Playing;
            }
        };
    }

    pub fn set_video_resolution(
        pipeline: &gst::Pipeline,
        cap_idx: usize,
        caps: Arc<Mutex<RawSourceCaps>>,
    ) -> Result<(), GstError> {
        let caps = &mut caps.lock().expect("Caps Mutex poisened");
        caps.set_resolution(cap_idx)?;
        update_video_settings(pipeline, caps)
    }

    pub fn set_framerate(
        pipeline: &gst::Pipeline,
        framerate_idx: usize,
        caps: Arc<Mutex<RawSourceCaps>>,
    ) -> Result<(), GstError> {
        let caps = &mut caps.lock().expect("Caps Mutex poisened");
        caps.set_framerate(framerate_idx)?;
        update_video_settings(pipeline, caps)
    }

    #[allow(unused)]
    fn get_bus(&self) -> Option<gst::Bus> {
        self.pipeline.bus()
    }
}

fn update_video_settings(
    pipeline: &gst::Pipeline,
    caps_guard: &mut MutexGuard<'_, RawSourceCaps>,
) -> Result<(), GstError> {
    pipeline
        .by_name("source")
        .ok_or(GstError::ElementNotFound)?
        .set_property("device", &caps_guard.get_current_device_path());

    let new_res = caps_guard.get_current_resolution();
    let fr = caps_guard.get_current_framerate()?;
    println!("Currently selected framerate: {fr:?}");
    let pipeline_caps = gst::Caps::builder("video/x-raw")
        .field("width", new_res.width)
        .field("height", new_res.height)
        .field("framerate", fr)
        .build();

    pipeline.send_event(FlushStart::new());
    pipeline.send_event(FlushStop::new(true));

    pipeline
        .by_name("filter")
        .ok_or(GstError::ElementNotFound)?
        .set_property("caps", &pipeline_caps);

    pipeline.send_event(Reconfigure::new());

    Ok(())
}

/// gets the current state of the pipeline (playing, paused, etc.)
#[inline]
fn get_current_state(pipeline: &gst::Pipeline) -> State {
    let (ret, state, pending) = pipeline.state(None);
    if ret.is_err() {
        eprintln!("State change failed. Continuing anyway.")
    }
    println!("Current state: {state:?}, pending state: {pending:?}.");
    state
}

/// sets the pipeline to the given state
fn set_current_state(
    pipeline: &gst::Pipeline,
    state: gst::State,
) -> Result<gst::StateChangeSuccess, gst::StateChangeError> {
    pipeline.set_state(state)
}
