use gst::{glib::object::Cast, prelude::GstBinExt};
use slint::{Image, Weak};

use crate::{image_pipeline::gstreamer::ImagePipeline, App};

pub struct FrameHandler {}

impl FrameHandler {
    pub fn init(pipeline: &gst::Pipeline, ui: Weak<App>) {
        // set up link of image pipeline output frames to GUI
        let new_frame_callback: fn(App, Image) = |ui: App, new_frame| {
            ui.set_video_frame(new_frame);
        };
        Self::register_frame_callback(&pipeline, ui, new_frame_callback)
            .expect("Failed to register new frame callback");

        // set up link of image pipeline video controls to GUI
    }

    fn register_frame_callback<AppHandle: slint::ComponentHandle + 'static>(
        pipeline: &gst::Pipeline,
        ui: Weak<AppHandle>,
        new_frame_cb: fn(AppHandle, Image),
    ) -> Result<(), ()> {
        let sink = pipeline
            .by_name("appsink0")
            .expect("Could not find appsink element in pipeline.")
            .downcast::<gst_app::AppSink>()
            .expect("Could not downcast Element to AppSink.");

        sink.set_callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_sample(move |appsink| {
                    let Ok(sample) = appsink.pull_sample() else {
                        println!("Failed to pull sample from appsink.");
                        return Err(gst::FlowError::Eos);
                    };

                    let Some(buffer) = sample.buffer() else {
                        println!("Sample has no buffer.");
                        return Err(gst::FlowError::Error);
                    };
                    let Ok(map) = buffer.map_readable() else {
                        println!("Failed to map buffer readable.");
                        return Err(gst::FlowError::Error);
                    };
                    let data = map.as_slice();

                    // Create an Image from the raw RGB data
                    let video_info =
                        gst_video::VideoInfo::from_caps(sample.caps().unwrap()).unwrap();
                    let pixel_buffer =
                        slint::SharedPixelBuffer::<slint::Rgb8Pixel>::clone_from_slice(
                            data,
                            video_info.width(),
                            video_info.height(),
                        );

                    ui.upgrade_in_event_loop(move |ui| {
                        new_frame_cb(ui, Image::from_rgb8(pixel_buffer));
                    })
                    .expect("Could not upgrade UI in event loop.");

                    Ok(gst::FlowSuccess::Ok)
                })
                .build(),
        );
        Ok(())
    }
}
