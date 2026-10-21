use send_wrapper::SendWrapper;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{AnalyserNode, AudioContext, DynamicsCompressorNode, MediaStream};

/// Event dispatched on `window` when the user talks while muted for at least
/// one second. Listen with `window.addEventListener("talk_while_muted", ...)`.
pub const TALK_WHILE_MUTED_EVENT: &str = "talk_while_muted";

/// Event dispatched on `window` after five seconds of sustained background
/// noise while the user is not talking.
pub const NOISE_DETECTED_EVENT: &str = "noise_detected";

/// How often the analyser is sampled, in milliseconds.
///
/// Also the unit that the sample-count thresholds in this module are measured
/// in, so a caller can convert them to wall-clock time.
pub const SAMPLE_INTERVAL_MS: u32 = 100;

/// FFT size for the analyser. 256 bins keeps the callback cheap enough to run
/// every 100 ms.
const FFT_SIZE: u32 = 256;

/// Average frequency-bin magnitude above which the user is considered talking.
const TALKING_THRESHOLD: f64 = 20.0;

/// Lower bound of the band treated as sustained background noise. Values above
/// [`TALKING_THRESHOLD`] are speech, not noise.
const NOISE_BAND_LOWER: f64 = 12.0;

/// Consecutive samples in the noise band before [`NOISE_DETECTED_EVENT`] fires.
const NOISE_SAMPLES_BEFORE_EVENT: u32 = 50;

/// Consecutive near-silent samples before `on_no_audio` fires.
pub const SILENCE_SAMPLES_BEFORE_NO_AUDIO: u32 = 100;

/// Consecutive samples of talking while muted before
/// [`TALK_WHILE_MUTED_EVENT`] fires.
const MUTED_TALK_SAMPLES_BEFORE_EVENT: u32 = 10;

/// Tracks microphone activity on a [`MediaStream`] and reports speech,
/// loudness and two warning conditions.
///
/// The monitor clones the stream's audio tracks and analyses the clone, so
/// speech is still detected while the original track is disabled by a mute
/// button. Sampled every [`SAMPLE_INTERVAL_MS`], it drives:
///
/// * `on_talking` — called when speech starts or stops.
/// * `on_level` — called with a normalised `0.0..=1.0` level; reports `0.0`
///   while muted so a level meter does not show the isolated analysis stream.
/// * `on_no_audio` — called once after [`SILENCE_SAMPLES_BEFORE_NO_AUDIO`]
///   samples of silence that follow no speech at all (a likely broken mic).
/// * [`TALK_WHILE_MUTED_EVENT`] and [`NOISE_DETECTED_EVENT`] — dispatched on
///   `window` for conditions the caller cannot observe through the callbacks.
///
/// All resources (audio context, interval, cloned tracks) are released on
/// `Drop`.
pub struct AudioMonitor {
    context: AudioContext,
    #[allow(dead_code)]
    analyser: AnalyserNode,
    _source: web_sys::MediaStreamAudioSourceNode,
    _closure: SendWrapper<Closure<dyn FnMut()>>,
    interval_id: i32,
    is_muted: SendWrapper<std::rc::Rc<std::cell::RefCell<bool>>>,
    is_noise_suppression_enabled: SendWrapper<std::rc::Rc<std::cell::RefCell<bool>>>,
    isolated_stream: MediaStream,
    compressor: Option<DynamicsCompressorNode>,
}

impl AudioMonitor {
    pub fn new(
        stream: &MediaStream,
        on_talking: Box<dyn FnMut(bool)>,
        on_level: Option<Box<dyn FnMut(f64)>>,
        mut on_no_audio: Option<Box<dyn FnMut()>>,
        noise_suppression: bool,
    ) -> Result<Self, JsValue> {
        // Bug 1 Fix: The original `stream`'s tracks are disabled when the user mutes themselves,
        // which sends silence to the Web Audio API. To detect talking while muted, we must
        // clone the audio track and keep it enabled, feeding the cloned stream to the AnalyserNode.
        let audio_tracks = stream.get_audio_tracks();
        let isolated_stream = MediaStream::new()?;

        for i in 0..audio_tracks.length() {
            let track_val = audio_tracks.get(i);
            if let Ok(track) = track_val.dyn_into::<web_sys::MediaStreamTrack>() {
                // We need to truly clone the JS MediaStreamTrack, not just the Rust reference
                let clone_fn = js_sys::Reflect::get(&track, &"clone".into())
                    .map_err(|_| JsValue::from_str("No clone method on MediaStreamTrack"))?;
                let clone_fn = clone_fn
                    .dyn_into::<js_sys::Function>()
                    .map_err(|_| JsValue::from_str("clone is not a function"))?;
                let cloned_val = clone_fn.call0(&track)?;
                let cloned_track = cloned_val
                    .dyn_into::<web_sys::MediaStreamTrack>()
                    .map_err(|_| JsValue::from_str("Clone did not return MediaStreamTrack"))?;

                // Ensure the cloned track remains enabled for local analysis even if original is disabled
                cloned_track.set_enabled(true);
                isolated_stream.add_track(&cloned_track);
            }
        }

        let context = AudioContext::new()?;
        let source = context.create_media_stream_source(&isolated_stream)?;
        let analyser = context.create_analyser()?;
        analyser.set_fft_size(FFT_SIZE);

        let mut compressor = None;
        if noise_suppression {
            let node = context.create_dynamics_compressor()?;
            node.threshold().set_value(-50.0);
            node.knee().set_value(40.0);
            node.ratio().set_value(12.0);
            node.attack().set_value(0.003);
            node.release().set_value(0.25);
            source.connect_with_audio_node(&node)?;
            node.connect_with_audio_node(&analyser)?;
            compressor = Some(node);
        } else {
            source.connect_with_audio_node(&analyser)?;
        }

        let mut callback = on_talking;
        let mut level_callback = on_level;
        let mut was_talking = false;
        let buffer_len = analyser.frequency_bin_count() as usize;
        let data_array = vec![0u8; buffer_len];

        let analyser_clone = analyser.clone();

        let is_muted = std::rc::Rc::new(std::cell::RefCell::new(false));
        let is_muted_clone = is_muted.clone();
        let is_noise_suppression_enabled =
            std::rc::Rc::new(std::cell::RefCell::new(noise_suppression));

        let mut silence_counter = 0;
        let mut no_audio_triggered = false;
        let mut has_ever_talked = false;
        let mut talk_while_muted_counter = 0;
        let mut notified_for_this_mute_cycle = false;
        let mut noise_counter = 0;
        let mut noise_triggered = false;

        let closure = Closure::wrap(Box::new(move || {
            let mut array = data_array.clone();
            analyser_clone.get_byte_frequency_data(&mut array);

            let sum: u32 = array.iter().map(|&x| x as u32).sum();
            let avg = sum as f64 / array.len() as f64;

            // Threshold for talking
            let is_talking = avg > TALKING_THRESHOLD;

            // Normalize average for level indicator (0.0 to 1.0).
            // Report 0.0 when muted so the indicator and any consumers
            // do not display real microphone activity from the isolated
            // analysis stream while the user is muted.
            let normalized_level = if *is_muted_clone.borrow() {
                0.0
            } else {
                (avg / 100.0).clamp(0.0, 1.0)
            };
            if let Some(ref mut cb) = level_callback {
                cb(normalized_level);
            }

            // Do not analyze or trigger callbacks while explicitly muted
            if *is_muted_clone.borrow() {
                // Talk While Muted feature
                if is_talking {
                    talk_while_muted_counter += 1;
                    if talk_while_muted_counter >= MUTED_TALK_SAMPLES_BEFORE_EVENT
                        && !notified_for_this_mute_cycle
                    {
                        notified_for_this_mute_cycle = true;
                        // The interval closure cannot reach the caller's state,
                        // so this condition is reported as a window event.
                        if let Some(window) = web_sys::window()
                            && let Ok(event) = web_sys::CustomEvent::new(TALK_WHILE_MUTED_EVENT)
                        {
                            let _ = window.dispatch_event(&event);
                        }
                    }
                } else {
                    talk_while_muted_counter = 0;
                }

                // Reset noise-detection state while muted so moderate background
                // audio captured on the isolated (always-enabled) analysis stream
                // does not accumulate toward the noise threshold and fire a
                // warning the user cannot act on. Also clear `noise_triggered`
                // so a legitimate noise warning can fire after unmuting.
                noise_counter = 0;
                noise_triggered = false;

                // If we are muted, we don't count silence towards the broken mic timeout.
                // We also ensure the "was_talking" state is cleanly suppressed.
                if was_talking {
                    was_talking = false;
                    callback(false);
                }
                return;
            } else {
                talk_while_muted_counter = 0;
                notified_for_this_mute_cycle = false;
            }

            if is_talking {
                has_ever_talked = true;
            }

            if is_talking != was_talking {
                was_talking = is_talking;
                callback(is_talking);
            }

            // Noise detection: Persistent moderate sound in the (12, 20] band
            // while the user is not "talking" (talking threshold is avg > 20.0).
            // This range targets background noise loud enough to be annoying
            // but not loud enough to be intentional speech. Since `!is_talking`
            // already constrains `avg <= 20.0`, an explicit upper bound check
            // here would be redundant.
            //
            // The counter accumulates only during the noise-sample band.
            // Anything outside that band (talking above 20, or quieter-than-
            // noise audio at or below 12) resets the counter so we require
            // *continuous* noise rather than letting intermittent episodes
            // separated by quieter gaps add up to the 5-second threshold.
            if avg > NOISE_BAND_LOWER && !is_talking {
                noise_counter += 1;
                if noise_counter > NOISE_SAMPLES_BEFORE_EVENT && !noise_triggered {
                    // NOISE_SAMPLES_BEFORE_EVENT * SAMPLE_INTERVAL_MS = 5 seconds
                    noise_triggered = true;
                    if let Some(window) = web_sys::window()
                        && let Ok(event) = web_sys::CustomEvent::new(NOISE_DETECTED_EVENT)
                    {
                        let _ = window.dispatch_event(&event);
                    }
                }
            } else if is_talking {
                noise_counter = 0;
                noise_triggered = false; // Reset when actually talking
            } else {
                // avg <= 12 and not talking — below the noise band.
                // Treat as a break in consistent noise.
                noise_counter = 0;
            }

            // If audio level is practically zero for a long time, and we haven't triggered yet
            if avg < 1.0 && !has_ever_talked {
                silence_counter += 1;
                if silence_counter > SILENCE_SAMPLES_BEFORE_NO_AUDIO && !no_audio_triggered {
                    // SILENCE_SAMPLES_BEFORE_NO_AUDIO * SAMPLE_INTERVAL_MS = 10 seconds
                    no_audio_triggered = true;
                    if let Some(cb) = on_no_audio.as_mut() {
                        cb();
                    }
                }
            } else {
                silence_counter = 0;
            }
        }) as Box<dyn FnMut()>);

        // Run interval
        let window = web_sys::window().unwrap();
        let interval_id = window.set_interval_with_callback_and_timeout_and_arguments_0(
            closure.as_ref().unchecked_ref(),
            SAMPLE_INTERVAL_MS as i32,
        )?;

        Ok(AudioMonitor {
            context,
            analyser,
            _source: source,
            _closure: SendWrapper::new(closure),
            interval_id,
            is_muted: SendWrapper::new(is_muted),
            is_noise_suppression_enabled: SendWrapper::new(is_noise_suppression_enabled),
            isolated_stream,
            compressor,
        })
    }

    pub fn set_muted(&self, muted: bool) {
        *self.is_muted.borrow_mut() = muted;
    }

    pub fn has_compressor(&self) -> bool {
        self.compressor.is_some()
    }

    pub fn set_noise_suppression(&self, enabled: bool) -> Result<(), JsValue> {
        let mut current = self.is_noise_suppression_enabled.borrow_mut();
        if *current == enabled {
            return Ok(());
        }

        if let Some(comp) = &self.compressor {
            // Compressor exists — adjust threshold to toggle bypass
            if enabled {
                comp.threshold().set_value(-50.0);
            } else {
                comp.threshold().set_value(0.0); // Effectively bypass
            }
            *current = enabled;
            Ok(())
        } else if enabled {
            // No compressor was created (monitor was built without noise suppression).
            // We cannot dynamically insert a DynamicsCompressorNode into the existing
            // audio graph without re-routing, so signal the caller to recreate the
            // AudioMonitor with the correct setting.
            Err(JsValue::from_str(
                "Cannot enable noise suppression: AudioMonitor was created without a compressor. Recreate the monitor.",
            ))
        } else {
            // Disabling when already disabled / no compressor — no-op is fine
            *current = enabled;
            Ok(())
        }
    }
}

impl Drop for AudioMonitor {
    fn drop(&mut self) {
        if let Some(window) = web_sys::window() {
            window.clear_interval_with_handle(self.interval_id);
        }
        let _ = self.context.close();

        // Ensure cloned tracks are explicitly stopped to release microphone
        let tracks = self.isolated_stream.get_tracks();
        for i in 0..tracks.length() {
            if let Ok(track) = tracks.get(i).dyn_into::<web_sys::MediaStreamTrack>() {
                track.stop();
            }
        }
    }
}
