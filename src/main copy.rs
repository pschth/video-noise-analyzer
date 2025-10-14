// // Prevent console window in addition to Slint window in Windows release builds when, e.g., starting the app via file manager. Ignored on other platforms.
// #![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// use gst::{prelude::*, MessageView};
// use std::{error::Error, io::pipe, rc::Rc, sync::Arc};

// use futures::stream::StreamExt;

// use slint::Model;

// slint::slint! {
//     export { Logic } from "ui/logic.slint";
//     export { App } from "ui/app-window.slint";
// }

// fn main() -> Result<(), Box<dyn Error>> {
//     slint::BackendSelector::new()
//         .backend_name("winit".into())
//         .require_opengl_es()
//         .select()
//         .expect("Unable to create Slint backend with OpenGL ES renderer");

//     let ui = App::new()?;

//     gst::init().unwrap();

//     let pipeline = gst::ElementFactory::make("playbin")
//         .property(
//             "uri",
//             "https://gstreamer.freedesktop.org/data/media/sintel_trailer-480p.webm",
//         )
//         .build()
//         .unwrap()
//         .downcast::<gst::Pipeline>()
//         .unwrap();

//     // Handle messages from the GStreamer pipeline bus.
//     // For most GStreamer objects with buses, you can use `while let Some(msg) = bus.next().await`
//     // inside an async closure passed to `slint::spawn_local` to read messages from the bus.
//     // However, that does not work for this pipeline's bus because gst::BusStream calls
//     // gst::Bus::set_sync_handler internally and gst::Bus::set_sync_handler also must be called
//     // on the pipeline's bus in the egl_integration. To work around this, send messages from the
//     // sync handler over an async channel, then receive them here.
//     let (bus_sender, mut bus_receiver) = futures::channel::mpsc::unbounded::<gst::Message>();
//     slint::spawn_local({
//         // GStreamer Objects are GLib Objects, so they are reference counted. Cloning increments
//         // the reference count, like cloning a std::rc::Rc.
//         let pipeline = pipeline.clone();
//         let ui = ui.as_weak().unwrap();
//         async move {
//             while let Some(msg) = bus_receiver.next().await {
//                 match msg.view() {
//                     MessageView::Buffering(b) => ui.set_buffering_percent(b.percent()),
//                     // Only update the `playing` property of the GUI in response to GStreamer's state changing
//                     // rather than updating it from GUI callbacks. This ensures that the state of the GUI stays
//                     // in sync with GStreamer.
//                     MessageView::StateChanged(s) => {
//                         if *s.src().unwrap() == pipeline {
//                             ui.global::<Logic>()
//                                 .set_playing(s.current() == gst::State::Playing);
//                         }
//                     }
//                     // When the file is finished playing, close the program.
//                     MessageView::Eos(..) => slint::quit_event_loop().unwrap(),
//                     MessageView::Error(err) => {
//                         eprintln!(
//                             "Error from {:?}: {} ({:?})",
//                             err.src().map(|s| s.path_string()),
//                             err.error(),
//                             err.debug()
//                         );
//                         slint::quit_event_loop().unwrap();
//                     }
//                     _ => (),
//                 }
//             }
//         }
//     })
//     .unwrap();

//     // If your application needs a GStreamer pipeline that is anything more complex
//     // than a single playbin element, you will need to link this gst::Element to some
//     // other gst::Element in your application code.
//     let _video_sink = slint_video_sink::init(&ui, &pipeline, bus_sender);

//     // set video sources to combobox model
//     let mut video_sources: Vec<slint::SharedString> = ui.get_video_sources().iter().collect();
//     video_sources.clear();
//     video_sources.push("Fucking Camera".into());

//     let video_sources = Rc::new(slint::VecModel::from(video_sources));

//     ui.set_video_sources(video_sources.clone().into());

//     ui.global::<Logic>().on_toggle_pause_play({
//         let ui_clone = ui.as_weak().unwrap();
//         move || {
//             let pipeline = pipeline.clone();
//             let state = pipeline.state(gst::ClockTime::NONE).1;
//             let logic = ui_clone.global::<Logic>();
//             match state {
//                 gst::State::Playing => {
//                     pipeline.set_state(gst::State::Paused).unwrap();
//                     logic.set_playing(false);
//                 }
//                 gst::State::Paused | gst::State::Ready | gst::State::Null => {
//                     pipeline.set_state(gst::State::Playing).unwrap();
//                     logic.set_playing(true);
//                 }
//                 _ => {}
//             }
//         }
//     });

//     ui.run()?;

//     Ok(())
// }
