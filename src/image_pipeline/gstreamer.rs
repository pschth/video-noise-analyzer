use std::{fmt::Display, sync::atomic::AtomicBool};

use gst::{debug, ffi::GstFraction, prelude::*, State};
use slint::{Model, ModelRc, SharedString, VecModel, Weak};
use thiserror::Error;

use crate::{
    image_pipeline::frame_handler::{self, FrameHandler},
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

#[derive(Debug)]
pub struct DeviceCapabilities {
    width: i32,
    height: i32,
    framerate: gst::List,
    device_path: String,
}

impl DeviceCapabilities {
    fn to_string_wo_framerate(&self) -> String {
        format!("{}: {}x{}", self.device_path, self.width, self.height)
    }
}

impl Display for DeviceCapabilities {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "video/x-raw, Resolution: {}x{}, Framerates: {:?}, Device Path: {}",
            self.width, self.height, self.framerate, self.device_path,
        )
    }
}

pub struct ImagePipeline {
    pipeline: gst::Pipeline,
    frame_handler: FrameHandler,
    caps: Vec<DeviceCapabilities>,
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
        let device_caps = Self::get_raw_source_caps();
        if device_caps.is_empty() {
            return Err(GstError::NoCapsFound);
        }

        // create source element
        let source = gst::ElementFactory::make("v4l2src")
            .name("source")
            .build()
            .expect("Could not create v4l2src element. Make sure the v4l2 plugin is installed.");
        source.set_property("device", &device_caps[0].device_path);

        // set video capabilities to the first available capability
        let framerate = device_caps
            .first()
            .unwrap()
            .framerate
            .first()
            .and_then(|v| v.get::<gst::Fraction>().ok())
            .expect("Could not get framerate from device capabilities.");

        let caps = gst::Caps::builder("video/x-raw")
            .field("width", device_caps.first().unwrap().width)
            .field("height", device_caps.first().unwrap().height)
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

        // link the video controls and the frame window to the image pipeline
        let frame_handler = ImagePipeline::link_with_gui(&pipeline, &ui, &device_caps);

        let img_pipeline = ImagePipeline {
            pipeline,
            caps: device_caps,
            frame_handler,
            shutdown_flag: AtomicBool::new(false),
            ui,
        };

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
        Self::set_state_cb(&self.pipeline, &self.ui, &self.frame_handler, state)
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

    fn link_with_gui(
        pipeline: &gst::Pipeline,
        ui: &Weak<App>,
        caps: &[DeviceCapabilities],
    ) -> FrameHandler {
        // set up link of image pipeline output frames to GUI
        let fh = FrameHandler::init(pipeline, ui.clone());

        // set up link of image pipeline video controls to GUI
        let pipeline = pipeline.clone();
        let gui_cb = ui.clone();
        let fh_cb = fh.clone();
        let ui = ui.upgrade().expect("Could not upgrade UI.");

        ui.on_toggle_play_pause(move || {
            Self::toggle_play_pause(&pipeline, &gui_cb, &fh_cb);
        });

        let available_sources: VecModel<SharedString> = caps
            .iter()
            .map(|s| SharedString::from(s.to_string_wo_framerate()))
            .collect();
        ui.set_video_sources(ModelRc::new(available_sources));

        let curr_source_index = ui.get_current_video_source() as usize;
        let framerates = VecModel::from_slice(&caps[curr_source_index].framerate);
        let framerates: VecModel<SharedString> = framerates
            .iter()
            .map(|f| {
                let frac = f.get::<gst::Fraction>().unwrap();
                SharedString::from(format!("{}/{}", frac.numer(), frac.denom()))
            })
            .collect();
        ui.set_framerates(ModelRc::new(framerates));

        fh
    }

    fn get_current_state(pipeline: &gst::Pipeline) -> State {
        let (ret, state, pending) = pipeline.state(None);
        if ret.is_err() {
            println!("State change failed. Continuing anyway.")
        }
        println!("Current state: {state:?}, pending state: {pending:?}.");
        state
    }

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

    fn set_video_properties(&self, cap_idx: usize) -> Result<(), GstError> {
        if cap_idx >= self.caps.len() {
            return Err(GstError::NoCapsFound);
        }
        let device_cap = &self.caps[cap_idx];

        self.pipeline
            .by_name("source")
            .ok_or(GstError::ElementNotFound)?
            .set_property("device", &device_cap.device_path);

        let caps = gst::Caps::builder("video/x-raw")
            .field("width", device_cap.width)
            .field("height", device_cap.height)
            .field("framerate", &device_cap.framerate)
            .build();

        self.pipeline
            .by_name("filter")
            .ok_or(GstError::ElementNotFound)?
            .set_property("caps", &caps);

        Ok(())
    }

    fn get_bus(&self) -> Option<gst::Bus> {
        self.pipeline.bus()
    }

    fn get_device_capabilities(&self) -> &Vec<DeviceCapabilities> {
        &self.caps
    }

    /// Queries all available v4l2 video source devices and their raw capabilities.
    fn get_raw_source_caps() -> Vec<DeviceCapabilities> {
        let mut device_caps: Vec<DeviceCapabilities> = vec![];

        // device monitor to list video sources
        let monitor = gst::DeviceMonitor::new();
        monitor.add_filter(Some("Video/Source"), None);
        monitor.start().expect("Could not start device monitor");

        for device in monitor.devices() {
            let display_name = device.display_name();
            // get linux video source device path, e.g. /dev/video0
            let properties = match device.properties() {
                None => {
                    println!("Device {display_name} has no properties.");
                    continue;
                }
                Some(props) => props,
            };
            let device_path = match properties.get::<String>("api.v4l2.path") {
                Err(_) => {
                    println!("Device {display_name} has no api.v4l2.path property.");
                    continue;
                }
                Ok(path) => path,
            };

            // print supported caps
            if let Some(caps_list) = device.caps() {
                println!("Available v4l2 caps:");
                for caps in caps_list.iter() {
                    if caps.name() != "video/x-raw" {
                        continue;
                    }

                    let width = caps.get::<i32>("width");
                    let height = caps.get::<i32>("height");
                    let framerate = caps.get::<gst::List>("framerate");

                    if let (Ok(width), Ok(height), Ok(framerate)) = (width, height, framerate) {
                        let device_cap = DeviceCapabilities {
                            width,
                            height,
                            framerate,
                            device_path: device_path.clone(),
                        };
                        device_caps.push(device_cap);
                    } else {
                        println!("  Could not get width/height/framerate/image_format for caps: {caps:?}");
                    }
                }
            } else {
                println!("Device {display_name} has no caps.");
            }
        }

        device_caps
    }
}
