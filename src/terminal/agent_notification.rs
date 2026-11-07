//! Local delivery boundary for Agent events and the settings preview.
#[cfg(test)]
#[derive(Default)]
pub(crate) struct RecordingDelivery {
    pub sounds: usize,
    pub hold_sound: bool,
    pub defer_dispatch: Option<smol::channel::Receiver<()>>,
    pub desktops: Vec<(String, String, gpui::EntityId, bool)>,
}
#[cfg(test)]
impl gpui::Global for RecordingDelivery {}

#[cfg(test)]
impl RecordingDelivery {
    /// Complete the held playback at the native audio boundary.
    pub(crate) fn complete_sound(cx: &mut gpui::App) {
        cx.default_global::<Playback>()
            .0
            .store(false, std::sync::atomic::Ordering::Release);
    }
}

use crate::core::config::{Config, NotifyMode};
use gpui::{App, EntityId};

pub(crate) fn deliver(title: &str, body: &str, pane: EntityId, source_focused: bool, cx: &mut App) {
    // The source window is leased by its view update, so include its active
    // state directly. Other windows include Settings and auxiliary windows.
    let focused_at_event = source_focused || app_focused(cx);
    if !focused_at_event {
        crate::ui::tray::background_agent_event(cx);
    }
    let cfg = cx.global::<Config>();
    let sound = allowed(cfg.agent_notification_sound, focused_at_event);
    let desktop = allowed(cfg.notify_on_command_finish, focused_at_event);
    if !sound && !desktop {
        return;
    }
    let title = title.to_owned();
    let body = body.to_owned();
    #[cfg(test)]
    let deferred = cx
        .default_global::<RecordingDelivery>()
        .defer_dispatch
        .clone();
    cx.spawn(async move |cx| {
        #[cfg(test)]
        if let Some(deferred) = deferred {
            let _ = deferred.recv().await;
        }
        let _ = cx.update(|cx| {
            let focused = app_focused(cx);
            let cfg = cx.global::<Config>();
            let sound = sound && allowed(cfg.agent_notification_sound, focused);
            let desktop = desktop && allowed(cfg.notify_on_command_finish, focused);
            if sound {
                play(cx);
            }
            if desktop {
                #[cfg(test)]
                {
                    cx.default_global::<RecordingDelivery>()
                        .desktops
                        .push((title, body, pane, true));
                }
                #[cfg(not(test))]
                super::remote::notify_agent_desktop_for_pane(&title, &body, pane);
            }
        });
    })
    .detach();
}

fn app_focused(cx: &mut App) -> bool {
    cx.windows().into_iter().any(|window| {
        window
            .update(cx, |_, window, _| window.is_window_active())
            .unwrap_or(false)
    })
}

fn allowed(mode: NotifyMode, app_focused: bool) -> bool {
    match mode {
        NotifyMode::Never => false,
        NotifyMode::Unfocused => !app_focused,
        NotifyMode::Always => true,
    }
}

pub(crate) fn preview(cx: &mut App) {
    play(cx);
}

#[derive(Default)]
struct Playback(std::sync::Arc<std::sync::atomic::AtomicBool>);
impl gpui::Global for Playback {}

fn play(cx: &mut App) {
    use std::sync::atomic::Ordering;
    let playing = cx.default_global::<Playback>().0.clone();
    if playing
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }
    #[cfg(test)]
    {
        let recording = cx.default_global::<RecordingDelivery>();
        recording.sounds += 1;
        if !recording.hold_sound {
            playing.store(false, Ordering::Release);
        }
        return;
    }
    #[cfg(not(test))]
    play_native(playing);
}

#[cfg(not(test))]
fn play_native(playing: std::sync::Arc<std::sync::atomic::AtomicBool>) {
    use std::sync::atomic::Ordering;
    let worker_playing = playing.clone();
    // Keep the playback slot until the native backend finishes. New events and
    // previews share this slot and never queue behind the current sound.
    if let Err(error) = std::thread::Builder::new()
        .name("tty7-agent-sound".into())
        .spawn(move || {
            if let Err(error) = play_wav() {
                log::warn!("failed to play Agent notification sound: {error:#}");
            }
            worker_playing.store(false, Ordering::Release);
        })
    {
        playing.store(false, Ordering::Release);
        log::warn!("failed to start Agent notification sound: {error}");
    }
}

#[cfg(not(test))]
static WAV: &[u8] = include_bytes!("../../assets/agent-notification.wav");

#[cfg(all(target_os = "windows", not(test)))]
fn play_wav() -> anyhow::Result<()> {
    use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_MEMORY, SND_NODEFAULT, SND_SYNC};
    // The static embedded buffer remains alive until synchronous playback ends.
    if unsafe {
        PlaySoundW(
            WAV.as_ptr().cast(),
            std::ptr::null_mut(),
            SND_MEMORY | SND_NODEFAULT | SND_SYNC,
        )
    } == 0
    {
        anyhow::bail!("PlaySoundW could not play the bundled WAV");
    }
    Ok(())
}

#[cfg(all(target_os = "macos", not(test)))]
fn play_wav() -> anyhow::Result<()> {
    use anyhow::Context as _;
    use objc2::{AnyThread as _, rc::autoreleasepool};
    use objc2_app_kit::NSSound;
    use objc2_foundation::NSData;

    // Construct and retain both Cocoa objects on this worker. NSSound accepts
    // in-memory WAV data and is usable from one thread without a delegate.
    autoreleasepool(|_| {
        let data = NSData::with_bytes(WAV);
        let sound = NSSound::initWithData(NSSound::alloc(), &data)
            .context("NSSound could not decode the bundled WAV")?;
        if !sound.play() {
            anyhow::bail!("NSSound could not start playback");
        }
        while sound.isPlaying() {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        Ok(())
    })
}

#[cfg(all(target_os = "linux", not(test)))]
fn play_wav() -> anyhow::Result<()> {
    use anyhow::Context as _;
    use libpulse_binding::{sample, stream::Direction};
    use libpulse_simple_binding::Simple;

    // Read RIFF chunks through a WAV decoder rather than assuming where the
    // data chunk starts. The bundled sound is signed 16-bit PCM.
    let mut reader = hound::WavReader::new(std::io::Cursor::new(WAV))
        .context("could not decode the bundled WAV")?;
    let wav_spec = reader.spec();
    anyhow::ensure!(
        wav_spec.sample_format == hound::SampleFormat::Int && wav_spec.bits_per_sample == 16,
        "the bundled WAV must contain 16-bit PCM"
    );
    let spec = sample::Spec {
        format: sample::Format::S16le,
        channels: u8::try_from(wav_spec.channels).context("invalid WAV channel count")?,
        rate: wav_spec.sample_rate,
    };
    anyhow::ensure!(spec.is_valid(), "invalid WAV playback format");
    let mut pcm = Vec::with_capacity(reader.len() as usize * 2);
    for sample in reader.samples::<i16>() {
        pcm.extend_from_slice(
            &sample
                .context("invalid PCM sample in the bundled WAV")?
                .to_le_bytes(),
        );
    }
    let stream = Simple::new(
        None,
        "ctty7",
        Direction::Playback,
        None,
        "Agent notification",
        &spec,
        None,
        None,
    )
    .context("could not connect to the desktop audio server")?;
    stream.write(&pcm).context("could not write Agent audio")?;
    stream.drain().context("could not finish Agent playback")?;
    Ok(())
}

#[cfg(all(
    not(any(target_os = "windows", target_os = "macos", target_os = "linux")),
    not(test)
))]
fn play_wav() -> anyhow::Result<()> {
    anyhow::bail!("Agent sound is unavailable on this platform")
}
