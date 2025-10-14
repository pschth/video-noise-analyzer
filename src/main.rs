mod image_pipeline;

use gst::{prelude::*, MessageView};
use std::{error::Error, io::pipe, rc::Rc, sync::Arc};

use futures::stream::StreamExt;

use slint::Model;

use image_pipeline::gstreamer::ImagePipeline;

slint::slint! {
    export { Logic } from "ui/logic.slint";
    export { App } from "ui/app-window.slint";
}

fn main() -> Result<(), Box<dyn Error>> {
    let device_caps = ImagePipeline::query_source_caps()?;

    return Ok(());

    let pipeline = ImagePipeline::new().expect("Failed to create image pipeline");

    pipeline
        .set_state(gst::State::Playing)
        .expect("Unable to set the pipeline to the `Playing` state");

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
