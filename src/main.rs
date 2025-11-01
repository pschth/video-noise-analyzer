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
    // let ui = App::new()?;

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

    let mut total_time = Duration::ZERO;
    let mut avg_time = Duration::ZERO;
    for i in 0..100 {
        let time_start = std::time::SystemTime::now();
        pipeline.get_sample();
        let elapsed = time_start.elapsed().unwrap();
        if i > 1 {
            total_time += elapsed;
            avg_time = total_time / (i - 1);
        }
        println!("Time elapsed for querying frame: {elapsed:?}, average so far: {avg_time:?}");
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
