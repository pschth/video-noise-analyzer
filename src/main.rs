mod image_pipeline;

use gst::{prelude::*, MessageView};
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

    let pipeline = Arc::new(pipeline);

    let bus = pipeline.get_bus().expect("Pipeline without bus");

    for _ in 0..100 {
        pipeline.print_sample_info();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

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

    Ok(())
}
