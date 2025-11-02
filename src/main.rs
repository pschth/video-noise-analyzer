mod image_pipeline;

use image_pipeline::gstreamer::ImagePipeline;
use std::error::Error;

slint::slint! {
    export { App } from "ui/app-window.slint";
}

fn main() -> Result<(), Box<dyn Error>> {
    let ui = App::new()?;

    let pipeline = ImagePipeline::new(ui.as_weak()).expect("Failed to create image pipeline");

    // let pipeline = Arc::new(pipeline);

    // let caps = pipeline.get_device_capabilities();
    // for (idx, cap) in caps.iter().enumerate() {
    //     println!("Device cap {idx}: {cap}");
    // }

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

    ui.run()?;

    let _ = pipeline.set_state(gst::State::Null);

    Ok(())
}
