use std::{
    fmt::Display,
    sync::{atomic::AtomicBool, Arc, Mutex, RwLock},
};

use gst::{debug, ffi::GstFraction, prelude::*, State};
use slint::{Model, ModelRc, SharedString, VecModel, Weak};
use thiserror::Error;
use v4l::device;

use crate::{
    image_pipeline::{
        device_caps::RawSourceCaps,
        frame_handler::{self, FrameHandler},
    },
    App,
};

#[derive(Debug, Error)]
pub enum GstError {
    #[error("Error: {0}")]
    Error(#[from] gst::glib::Error),

    #[error("Bool error: {0}")]
    BoolError(#[from] gst::glib::BoolError),

    #[error("No capabilities found error")]
    NoCapsFound,

    #[error("Element not found error")]
    ElementNotFound,
}

#[derive(Debug, Error)]
pub enum UiError {
    #[error("UI already initialized.")]
    AlreadyInitialized,
}

pub struct ImagePipeline {
    pipeline: gst::Pipeline,
    frame_handler: Option<FrameHandler>,
    caps: Arc<Mutex<RawSourceCaps>>,
    shutdown_flag: AtomicBool,
    ui: Weak<App>,
}

impl Drop for ImagePipeline {
    fn drop(&mut self) {
        self.shutdown_flag
            .store(true, std::sync::atomic::Ordering::Relaxed);
        self.pipeline.set_state(gst::State::Null).unwrap();
    }
}

impl ImagePipeline {
    pub fn new(ui: Weak<App>) -> Result<Self, GstError> {
        gst::init()?;
        let caps_obj = RawSourceCaps::new();
        let device_caps = caps_obj.get_caps();
        if device_caps.is_empty() {
            return Err(GstError::NoCapsFound);
        }

        // create source element
        let curr_cap = &device_caps[caps_obj.curr_cap_idx];
        let source = gst::ElementFactory::make("v4l2src")
            .name("source")
            .build()
            .expect("Could not create v4l2src element. Make sure the v4l2 plugin is installed.");
        source.set_property("device", &curr_cap.device_path);

        // set video capabilities to the first available capability
        let curr_framerate = &curr_cap.framerates[caps_obj.curr_framerate_idx];
        let framerate: gst::Fraction = curr_framerate
            .get::<gst::Fraction>()
            .expect("Could not get framerate from device capabilities.");

        let caps = gst::Caps::builder("video/x-raw")
            .field("width", curr_cap.width)
            .field("height", curr_cap.height)
            .field("framerate", framerate)
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
        let rgb_caps = gst::Caps::builder("video/x-raw")
            .field("format", "RGB")
            .build();

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
        pipeline.add_many([&source, &capsfilter, &convert, &rgb_filter, &sink])?;
        gst::Element::link_many([&source, &capsfilter, &convert, &rgb_filter, &sink])?;

        let mut img_pipeline = ImagePipeline {
            pipeline,
            caps: Arc::new(Mutex::new(caps_obj)),
            frame_handler: None,
            shutdown_flag: AtomicBool::new(false),
            ui,
        };

        // link the video controls and the frame window to the image pipeline
        img_pipeline.link_with_gui().map_err(|e| {
            println!("Failed to link image pipeline with GUI: {}", e);
            GstError::Error(gst::glib::Error::new(
                gst::glib::FileError::Failed,
                "Failed to link image pipeline with GUI.",
            ))
        })?;

        // set image pipeline to pause state if possible
        img_pipeline
            .set_state(State::Paused)
            .expect("Could not set pipeline into pause state.");

        Ok(img_pipeline)
    }

    /// sets the pipeline to the given state
    pub fn set_state(
        &self,
        state: gst::State,
    ) -> Result<gst::StateChangeSuccess, gst::StateChangeError> {
        let Some(fh) = &self.frame_handler else {
            println!("Frame handler not initialized.");
            return Err(gst::StateChangeError);
        };
        Self::set_state_cb(&self.pipeline, &self.ui, fh, state)
    }

    pub fn set_state_cb(
        pipeline: &gst::Pipeline,
        ui: &Weak<App>,
        frame_handler: &FrameHandler,
        state: gst::State,
    ) -> Result<gst::StateChangeSuccess, gst::StateChangeError> {
        match pipeline.set_state(state) {
            Ok(r) => {
                let ui = ui.upgrade().expect("Could not upgrade UI.");
                match state {
                    State::Playing => ui.set_playing(true),
                    _ => {
                        ui.set_playing(false);
                        frame_handler.display_pause_image();
                    }
                };
                Ok(r)
            }
            Err(r) => {
                println!("Failed to set state");
                Err(r)
            }
        }
    }

    /// links the gstreamer image pipeline to the GUI controls and frame display
    fn link_with_gui(&mut self) -> Result<(), UiError> {
        // set up link of image pipeline output frames to GUI
        if self.frame_handler.is_some() {
            return Err(UiError::AlreadyInitialized);
        }
        let ui = Arc::new(self.ui.clone());
        self.frame_handler
            .replace(FrameHandler::init(&self.pipeline, ui.clone()));

        // set up link of image pipeline video controls to GUI
        let pipeline_arc = Arc::new(self.pipeline.clone());
        let ui_weak_arc = ui.clone();
        let frame_handler_arc = Arc::new(self.frame_handler.clone().unwrap());
        let caps_arc = self.caps.clone();
        let ui_arc = Arc::new(ui.upgrade().expect("Could not upgrade UI."));

        // set up callbacks for video controls
        ui_arc.on_toggle_play_pause({
            let pipeline_arc = pipeline_arc.clone();
            move || {
                Self::toggle_play_pause(
                    pipeline_arc.as_ref(),
                    ui_weak_arc.as_ref(),
                    frame_handler_arc.as_ref(),
                );
            }
        });

        let ui_clone = ui_arc.clone();
        ui_arc.on_selected_video_source({
            let pipeline_arc = pipeline_arc.clone();
            let caps_arc = caps_arc.clone();
            move |value| {
                let selected_idx = ui_clone.get_current_video_source() as usize;
                println!("Selected video source: {value}, index: {selected_idx}");

                if Self::set_video_resolution(pipeline_arc.as_ref(), selected_idx, caps_arc.clone())
                    .is_err()
                {
                    println!("Failed to set video properties for selected source.");
                };
            }
        });

        let ui_clone = ui_arc.clone();
        ui_arc.on_selected_framerate({
            let pipeline_arc = pipeline_arc.clone();
            move |value| {
                let selected_idx = ui_clone.get_curr_fps() as usize;
                println!("Selected framerate: {value}, index: {selected_idx}");

                if Self::set_framerate(pipeline_arc.as_ref(), selected_idx, caps_arc.clone())
                    .is_err()
                {
                    println!("Failed to set video properties for selected source.");
                };
            }
        });

        let cap_lock = self.caps.lock().expect("Could not acquire lock");
        let cap_vec = cap_lock.get_caps();
        let available_sources: VecModel<SharedString> = cap_vec
            .iter()
            .map(|s| SharedString::from(s.to_string_wo_framerate()))
            .collect();
        ui_arc.set_video_sources(ModelRc::new(available_sources));

        let curr_source_index = ui_arc.get_current_video_source() as usize;
        let framerates = VecModel::from_slice(&cap_vec[curr_source_index].framerates);
        let framerates: VecModel<SharedString> = framerates
            .iter()
            .map(|f| {
                let frac = f.get::<gst::Fraction>().unwrap();
                SharedString::from(format!("{}/{}", frac.numer(), frac.denom()))
            })
            .collect();
        ui_arc.set_framerates(ModelRc::new(framerates));

        Ok(())
    }

    /// gets the current state of the pipeline (playing, paused, etc.)
    fn get_current_state(pipeline: &gst::Pipeline) -> State {
        let (ret, state, pending) = pipeline.state(None);
        if ret.is_err() {
            println!("State change failed. Continuing anyway.")
        }
        println!("Current state: {state:?}, pending state: {pending:?}.");
        state
    }

    /// toggles between play and pause states of the pipeline
    fn toggle_play_pause(pipeline: &gst::Pipeline, ui: &Weak<App>, frame_handler: &FrameHandler) {
        match Self::get_current_state(pipeline) {
            State::Playing => {
                if Self::set_state_cb(pipeline, ui, frame_handler, State::Paused).is_err() {
                    println!("Failed to pause video.");
                };
            }
            _ => {
                if Self::set_state_cb(pipeline, ui, frame_handler, State::Playing).is_err() {
                    println!("Failed to play video.");
                };
            }
        };
    }

    fn set_video_resolution(
        pipeline: &gst::Pipeline,
        cap_idx: usize,
        caps: Arc<Mutex<RawSourceCaps>>,
    ) -> Result<(), GstError> {
        let caps = &mut caps.lock().expect("Caps Mutex poisened");
        if cap_idx >= caps.len() {
            return Err(GstError::NoCapsFound);
        }
        let device_cap = &caps.get_caps()[cap_idx];

        pipeline
            .by_name("source")
            .ok_or(GstError::ElementNotFound)?
            .set_property("device", &device_cap.device_path);

        let pipeline_caps = gst::Caps::builder("video/x-raw")
            .field("width", device_cap.width)
            .field("height", device_cap.height)
            .build();

        pipeline
            .by_name("filter")
            .ok_or(GstError::ElementNotFound)?
            .set_property("caps", &pipeline_caps);

        caps.curr_cap_idx = cap_idx;

        Ok(())
    }

    fn set_framerate(
        pipeline: &gst::Pipeline,
        framerate_idx: usize,
        caps: Arc<Mutex<RawSourceCaps>>,
    ) -> Result<(), GstError> {
        let caps = &mut caps.lock().expect("Caps Mutex poisened");
        let device_cap = &caps.get_caps()[caps.curr_cap_idx];

        if framerate_idx >= device_cap.framerates.len() {
            println!(
                "Framerate index {framerate_idx} out of bounds. Max is {}.",
                device_cap.framerates.len() - 1
            );
            return Err(GstError::NoCapsFound);
        }

        pipeline
            .by_name("source")
            .ok_or(GstError::ElementNotFound)?
            .set_property("device", &device_cap.device_path);

        let framerate = &device_cap.framerates[framerate_idx];
        let framerate: gst::Fraction = framerate.get::<gst::Fraction>().map_err(|_| {
            println!("Could not convert framerate from device capabilities.");
            GstError::NoCapsFound
        })?;

        let pipeline_caps = gst::Caps::builder("video/x-raw")
            .field("framerate", framerate)
            .build();

        pipeline
            .by_name("filter")
            .ok_or(GstError::ElementNotFound)?
            .set_property("caps", &pipeline_caps);

        caps.curr_framerate_idx = framerate_idx;

        Ok(())
    }

    fn get_bus(&self) -> Option<gst::Bus> {
        self.pipeline.bus()
    }

    fn get_device_capabilities(&self) -> &Arc<Mutex<RawSourceCaps>> {
        &self.caps
    }
}
