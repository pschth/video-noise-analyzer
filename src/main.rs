mod image_pipeline;

use gst::{prelude::*, MessageView};
use std::error::Error;
use std::sync::Arc;

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
    let pipeline_clone = Arc::clone(&pipeline);
    std::thread::spawn(move || {
        let mut cap_idx = 0;
        let mut now = std::time::Instant::now();
        loop {
            if now.elapsed().as_secs() >= 5 {
                cap_idx += 1;
                println!("Switching to capability index: {}", cap_idx);
                pipeline_clone.set_video_properties(cap_idx).unwrap();
                now = std::time::Instant::now();
            }
        }
    });

    let bus = pipeline.get_bus().expect("Pipeline without bus");
    for msg in bus.iter_timed(gst::ClockTime::NONE) {
        match msg.view() {
            MessageView::Eos(..) => {
                println!("End of stream");
                break;
            }
            MessageView::Error(err) => {
                eprintln!(
                    "Error from {:?}: {} ({:?})",
                    err.src().map(|s| s.path_string()),
                    err.error(),
                    err.debug()
                );
                break;
            }
            _ => {}
        }
    }

    pipeline.set_state(gst::State::Null).unwrap();

    Ok(())
}
