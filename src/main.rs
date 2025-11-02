mod image_pipeline;

use gst::ffi::GstPipelineClass;
use gst::{prelude::*, MessageView};
use slint::Image;
use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use image_pipeline::gstreamer::ImagePipeline;

slint::slint! {
    export { Logic } from "ui/logic.slint";
    export { App } from "ui/app-window.slint";
}

fn main() -> Result<(), Box<dyn Error>> {
    let pipeline = ImagePipeline::new().expect("Failed to create image pipeline");

    pipeline
        .set_state(gst::State::Playing)
        .expect("Unable to set the pipeline to the `Playing` state");

    // let pipeline = Arc::new(pipeline);

    let caps = pipeline.get_device_capabilities();
    for (idx, cap) in caps.iter().enumerate() {
        println!("Device cap {idx}: {cap}");
    }

    // let bus = pipeline.get_bus().expect("Pipeline without bus");

    // for msg in bus.iter_timed(gst::ClockTime::NONE) {
    //     match msg.view() {
    //         MessageView::Eos(..) => {
    //             println!("End of stream");
    //             break;
    //         }
    //         MessageView::Error(err) => {
    //             eprintln!(
    //                 "Error from {:?}: {} ({:?})",
    //                 err.src().map(|s| s.path_string()),
    //                 err.error(),
    //                 err.debug()
    //             );
    //             break;
    //         }
    //         _ => {}
    //     }
    // }

    let ui = App::new()?;

    let new_frame_callback = |app: App, new_frame| {
        app.set_video_frame(new_frame);
    };
    pipeline
        .register_frame_callback(&ui, new_frame_callback)
        .expect("Failed to register new frame callback");

    ui.run()?;
    println!("UI has exited.");

    let _ = pipeline.set_state(gst::State::Null);

    Ok(())
}
