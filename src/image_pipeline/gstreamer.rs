use std::{
    fmt::Display,
    sync::{atomic::AtomicBool, mpsc, Arc},
    time::Duration,
};

use futures::lock::Mutex;
use gst::{prelude::*, BufferMap, Sample};
use gst_app::AppSink;
use gst_video::video_codec_state::Readable;
use thiserror::Error;

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
    caps: Vec<DeviceCapabilities>,
    sample_rx: mpsc::Receiver<Sample>,
    shutdown_flag: AtomicBool,
}

impl Drop for ImagePipeline {
    fn drop(&mut self) {
        self.shutdown_flag
            .store(true, std::sync::atomic::Ordering::Relaxed);
        self.pipeline.set_state(gst::State::Null).unwrap();
    }
}

impl ImagePipeline {
    pub fn new() -> Result<Self, GstError> {
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
        // set callback to send new samples through channel to ImagePipeline object
        let (sample_tx, sample_rx) = mpsc::channel::<Sample>();
        sink.set_callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_sample(move |appsink| {
                    let sample = appsink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                    sample_tx
                        .send(sample)
                        .expect("Failed to send sample through channel");
                    Ok(gst::FlowSuccess::Ok)
                })
                .build(),
        );

        // build the pipeline
        let pipeline = gst::Pipeline::new();
        let sink: gst::Element = sink.upcast();
        pipeline.add_many([&source, &capsfilter, &convert, &rgb_filter, &sink])?;
        gst::Element::link_many([&source, &capsfilter, &convert, &rgb_filter, &sink])?;

        Ok(ImagePipeline {
            pipeline,
            caps: device_caps,
            sample_rx,
            shutdown_flag: AtomicBool::new(false),
        })
    }

    pub fn get_sample(&self) -> Option<Sample> {
        match self.sample_rx.recv_timeout(Duration::from_secs(1)) {
            Ok(sample) => Some(sample),
            Err(_) => {
                println!("No sample received within 1 second.");
                None
            }
        }

        // let sample = match self.sample_rx.recv_timeout(Duration::from_secs(1)) {
        //     Ok(sample) => Some(sample),
        //     Err(_) => {
        //         println!("No sample received within 1 second.");
        //         None
        //     }
        // }?;

        // let buffer = sample.buffer().expect("Failed to get buffer from sample");
        // let map = buffer
        //     .map_readable()
        //     .expect("Failed to map buffer readable");
        // let data = map.as_slice();
        // let data_rms = data.iter().map(|&b| (b as f64).powi(2)).sum::<f64>() / data.len() as f64;
        // println!(
        //     "Sample rms: {data_rms}, slice length: {}, map size: {}",
        //     data.len(),
        //     map.size()
        // );
        // None
    }

    /// sets the pipeline to the given state
    pub fn set_state(
        &self,
        state: gst::State,
    ) -> Result<gst::StateChangeSuccess, gst::StateChangeError> {
        self.pipeline.set_state(state)
    }

    pub fn set_video_properties(&self, cap_idx: usize) -> Result<(), GstError> {
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

    pub fn get_bus(&self) -> Option<gst::Bus> {
        self.pipeline.bus()
    }

    pub fn get_device_capabilities(&self) -> &Vec<DeviceCapabilities> {
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
