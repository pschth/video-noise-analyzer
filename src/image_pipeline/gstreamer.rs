use gst::prelude::*;
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
    format: String,
    device_path: String,
}

pub struct ImagePipeline {
    pipeline: gst::Pipeline,
    caps: Vec<DeviceCapabilities>,
}

impl ImagePipeline {
    pub fn new() -> Result<Self, GstError> {
        gst::init()?;
        let device_caps = Self::get_source_caps();
        if device_caps.is_empty() {
            return Err(GstError::NoCapsFound);
        }

        // create source element
        let source = match gst::ElementFactory::make("v4l2src").name("source").build() {
            Ok(element) => element,
            Err(_) => {
                eprintln!(
                    "Could not create v4l2src element. Make sure the v4l2 plugin is installed."
                );
                return Err(GstError::ElementNotFound);
            }
        };
        source.set_property("device", &device_caps[0].device_path);

        // set video capabilities to the first available capability
        let framerate = match device_caps
            .first()
            .unwrap()
            .framerate
            .first()
            .and_then(|v| v.get::<gst::Fraction>().ok())
        {
            Some(fr) => fr,
            None => {
                eprintln!("Could not get framerate from device capabilities.");
                return Err(GstError::NoCapsFound);
            }
        };
        let caps = gst::Caps::builder("image/jpeg")
            .field("width", device_caps.first().unwrap().width)
            .field("height", device_caps.first().unwrap().height)
            .field("framerate", framerate)
            .build();

        let capsfilter = match gst::ElementFactory::make("capsfilter")
            .name("filter")
            .build()
        {
            Ok(element) => element,
            Err(_) => {
                eprintln!("Could not create capsfilter element.");
                return Err(GstError::ElementNotFound);
            }
        };
        capsfilter.set_property("caps", &caps);

        // set up the rest of the pipeline
        let decoder = match gst::ElementFactory::make("jpegdec").name("decoder").build() {
            Ok(element) => element,
            Err(_) => {
                eprintln!(
                    "Could not create jpegdec element. Make sure the jpeg plugin is installed."
                );
                return Err(GstError::ElementNotFound);
            }
        };

        let convert = match gst::ElementFactory::make("videoconvert")
            .name("convert")
            .build()
        {
            Ok(element) => element,
            Err(_) => {
                eprintln!("Could not create videoconvert element.");
                return Err(GstError::ElementNotFound);
            }
        };

        let sink = match gst::ElementFactory::make("autovideosink").build() {
            Ok(element) => element,
            Err(_) => {
                eprintln!("Could not create autovideosink element.");
                return Err(GstError::ElementNotFound);
            }
        };

        let pipeline = gst::Pipeline::new();
        pipeline.add_many([&source, &capsfilter, &decoder, &convert, &sink])?;
        gst::Element::link_many([&source, &capsfilter, &decoder, &convert, &sink])?;

        Ok(Self {
            pipeline,
            caps: device_caps,
        })
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

        let caps = gst::Caps::builder(&device_cap.format)
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

    /// Queries all available v4l2 video source devices and their capabilities.
    fn get_source_caps() -> Vec<DeviceCapabilities> {
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

        device_caps
    }
}
