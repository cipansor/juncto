use send_wrapper::SendWrapper;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, HtmlVideoElement, MediaStream};

pub struct VideoProcessor {
    #[allow(dead_code)]
    canvas: HtmlCanvasElement,
    #[allow(dead_code)]
    context: CanvasRenderingContext2d,
    video: HtmlVideoElement,
    #[allow(dead_code)]
    _closure: SendWrapper<Closure<dyn FnMut()>>,
    interval_id: i32,
    #[allow(dead_code)]
    mode: SendWrapper<Rc<RefCell<String>>>,
}

impl VideoProcessor {
    pub fn set_mode(&self, new_mode: String) {
        *self.mode.borrow_mut() = new_mode;
    }

    pub fn new(stream: &MediaStream, initial_mode: String) -> Result<(Self, MediaStream), JsValue> {
        let window = web_sys::window().unwrap();
        let document = window.document().unwrap();
        let canvas = document
            .create_element("canvas")?
            .dyn_into::<HtmlCanvasElement>()?;
        let context = canvas
            .get_context("2d")?
            .unwrap()
            .dyn_into::<CanvasRenderingContext2d>()?;
        let video = document
            .create_element("video")?
            .dyn_into::<HtmlVideoElement>()?;

        video.set_src_object(Some(stream));
        video.set_muted(true);
        let _ = video.play();

        let mode = Rc::new(RefCell::new(initial_mode));
        let mode_clone = mode.clone();
        let canvas_clone = canvas.clone();
        let context_clone = context.clone();
        let video_clone = video.clone();
        let last_width: Rc<RefCell<u32>> = Rc::new(RefCell::new(0));
        let last_height: Rc<RefCell<u32>> = Rc::new(RefCell::new(0));

        // Resolve and invoke captureStream before registering the interval. If
        // this fails the constructor returns early, and a timer registered first
        // would outlive the closure it points at: the browser would keep calling
        // freed memory every 33 ms.
        //
        // capture_stream is not directly in web_sys for all browsers, use Reflect as fallback
        let capture_stream = js_sys::Reflect::get(&canvas, &"captureStream".into())
            .ok()
            .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
            .ok_or_else(|| JsValue::from_str("Canvas captureStream not supported"))?;
        let processed_stream = capture_stream.call0(&canvas)?.dyn_into::<MediaStream>()?;

        let closure = Closure::wrap(Box::new(move || {
            let width = video_clone.video_width() as f64;
            let height = video_clone.video_height() as f64;
            if width == 0.0 || height == 0.0 {
                return;
            }

            // Only update canvas dimensions when the video resolution changes.
            // Calling set_width/set_height clears the canvas buffer, so doing it
            // every frame (~30fps) wastes CPU on an unnecessary buffer reset.
            let w = width as u32;
            let h = height as u32;
            if *last_width.borrow() != w || *last_height.borrow() != h {
                canvas_clone.set_width(w);
                canvas_clone.set_height(h);
                *last_width.borrow_mut() = w;
                *last_height.borrow_mut() = h;
            }

            let current_mode = mode_clone.borrow();
            match current_mode.as_str() {
                "blur" => {
                    context_clone.set_filter("blur(5px)");
                    let _ =
                        context_clone.draw_image_with_html_video_element(&video_clone, 0.0, 0.0);
                }
                "image" => {
                    // Draw a placeholder background color (representing an image)
                    context_clone.set_filter("none");
                    let style = wasm_bindgen::JsValue::from_str("#004400");
                    #[allow(deprecated)]
                    context_clone.set_fill_style(&style);
                    context_clone.fill_rect(0.0, 0.0, width, height);
                    let _ =
                        context_clone.draw_image_with_html_video_element(&video_clone, 0.0, 0.0);
                }
                _ => {
                    context_clone.set_filter("none");
                    let _ =
                        context_clone.draw_image_with_html_video_element(&video_clone, 0.0, 0.0);
                }
            }
        }) as Box<dyn FnMut()>);

        let interval_id = window.set_interval_with_callback_and_timeout_and_arguments_0(
            closure.as_ref().unchecked_ref(),
            33, // ~30fps
        )?;

        // Canvas captureStream only contains video tracks. Copy audio tracks from the
        // original stream so that WebRTC peers still receive audio and mute/unmute
        // operations continue to work when a virtual background is active.
        let audio_tracks = stream.get_audio_tracks();
        for i in 0..audio_tracks.length() {
            let track = audio_tracks.get(i);
            if let Ok(track) = track.dyn_into::<web_sys::MediaStreamTrack>() {
                processed_stream.add_track(&track);
            }
        }

        Ok((
            Self {
                canvas,
                context,
                video,
                _closure: SendWrapper::new(closure),
                interval_id,
                mode: SendWrapper::new(mode),
            },
            processed_stream,
        ))
    }
}

impl Drop for VideoProcessor {
    fn drop(&mut self) {
        if let Some(window) = web_sys::window() {
            window.clear_interval_with_handle(self.interval_id);
        }
        self.video.set_src_object(None);
    }
}
