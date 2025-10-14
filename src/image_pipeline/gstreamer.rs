use gst::prelude::*;
use thiserror::Error;
use v4l::{prelude::*, video::Capture};

#[derive(Debug, Error)]
pub enum GstError {
    #[error("Error: {0}")]
    Error(#[from] gst::glib::Error),

    #[error("Bool error: {0}")]
    BoolError(#[from] gst::glib::BoolError),
}

#[derive(Debug)]
pub struct DeviceCapabilities {
    width: i32,
    height: i32,
    framerate: gst::List,
    format: String,
    device_path: String,
}

pub struct ImagePipeline {
    pipeline: gst::Pipeline,
}

impl ImagePipeline {
    /// Queries all available v4l2 video source devices and their capabilities.
    pub fn query_source_caps() -> Result<Vec<DeviceCapabilities>, Box<dyn std::error::Error>> {
        gst::init()?;

        let mut device_caps: Vec<DeviceCapabilities> = vec![];

        // device monitor to list video sources
        let monitor = gst::DeviceMonitor::new();
        monitor.add_filter(Some("Video/Source"), None);
        monitor.start()?;

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

            println!("Device: {display_name}, device path: {device_path}");

            // print supported caps
            if let Some(caps_list) = device.caps() {
                println!("Available v4l2 caps:");
                for caps in caps_list.iter() {
                    let width = caps.get::<i32>("width");
                    let height = caps.get::<i32>("height");
                    let framerate = caps.get::<gst::List>("framerate");

                    if let (Ok(width), Ok(height), Ok(framerate)) = (width, height, framerate) {
                        let device_cap = DeviceCapabilities {
                            width,
                            height,
                            framerate,
                            format: caps.name().to_string(),
                            device_path: device_path.clone(),
                        };
                        println!("  {device_cap:?}");
                        device_caps.push(device_cap);
                    } else {
                        println!("  Could not get width/height/framerate for caps: {caps:?}");
                    }
                }
            } else {
                println!("Device {display_name} has no caps.");
            }
        }

        Ok(vec![])
    }

    pub fn new() -> Result<Self, GstError> {
        gst::init()?;

        let source = gst::ElementFactory::make("v4l2src")
            .name("source")
            .build()?;

        let capsfilter = gst::ElementFactory::make("capsfilter")
            .name("filter")
            .build()?;

        let caps = gst::Caps::builder("image/jpeg")
            .field("width", 1920)
            .field("height", 1080)
            .field("framerate", gst::Fraction::new(30, 1))
            .build();
        capsfilter.set_property("caps", &caps);

        let decoder = gst::ElementFactory::make("jpegdec")
            .name("decoder")
            .build()
            .expect("Could not create decoder element.");

        let convert = gst::ElementFactory::make("videoconvert")
            .name("convert")
            .build()
            .expect("Could not create convert element.");

        let sink = gst::ElementFactory::make("autovideosink")
            .build()
            .expect("Could not create sink element.");

        let pipeline = gst::Pipeline::new();
        pipeline
            .add_many([&source, &capsfilter, &decoder, &convert, &sink])
            .unwrap();
        gst::Element::link_many([&source, &capsfilter, &decoder, &convert, &sink]).unwrap();

        Ok(Self { pipeline })
    }

    pub fn set_state(
        &self,
        state: gst::State,
    ) -> Result<gst::StateChangeSuccess, gst::StateChangeError> {
        self.pipeline.set_state(state)
    }

    pub fn get_bus(&self) -> Option<gst::Bus> {
        self.pipeline.bus()
    }
}
