use crate::app::ports::outbound::clipboard::{ClipboardError, ClipboardWriter};

/// Clipboard writer backed by `arboard`.
///
/// On Linux (X11 and Wayland) there is no central clipboard store: copying
/// only makes this process the owner of the selection, and the data lives in
/// our memory until another client asks for it at paste time. Dropping the
/// `arboard::Clipboard` right after `set_text` therefore empties the clipboard
/// almost immediately (unless a clipboard manager grabs it first). To stay the
/// selection owner, a single `arboard::Clipboard` is kept alive for the whole
/// session on a dedicated thread (the handle is not `Send` on every platform),
/// and copy requests are sent to it over a channel.
pub struct ArboardClipboard {
    #[cfg(not(target_os = "android"))]
    tx: std::sync::mpsc::Sender<owner::Request>,
}

impl ArboardClipboard {
    pub fn new() -> Self {
        Self {
            #[cfg(not(target_os = "android"))]
            tx: owner::spawn(),
        }
    }
}

impl Default for ArboardClipboard {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(not(target_os = "android"))]
impl ClipboardWriter for ArboardClipboard {
    fn copy_text(&self, content: &str) -> Result<(), ClipboardError> {
        let (reply_tx, reply_rx) = std::sync::mpsc::sync_channel(1);
        self.tx
            .send((content.to_owned(), reply_tx))
            .map_err(|_| owner::gone())?;
        reply_rx.recv().map_err(|_| owner::gone())?
    }
}

#[cfg(target_os = "android")]
impl ClipboardWriter for ArboardClipboard {
    fn copy_text(&self, _content: &str) -> Result<(), ClipboardError> {
        Err(ClipboardError::Unavailable("Clipboard unavailable".into()))
    }
}

#[cfg(not(target_os = "android"))]
mod owner {
    use std::sync::mpsc;

    use super::ClipboardError;

    pub(super) type Request = (String, mpsc::SyncSender<Result<(), ClipboardError>>);

    pub(super) fn gone() -> ClipboardError {
        ClipboardError::Unavailable("Clipboard thread stopped".into())
    }

    pub(super) fn spawn() -> mpsc::Sender<Request> {
        let (tx, rx) = mpsc::channel::<Request>();
        std::thread::Builder::new()
            .name("clipboard".into())
            .spawn(move || {
                // Opened lazily so a headless session still starts fine and
                // reports the error on first copy; retried until it succeeds.
                let mut clipboard: Option<arboard::Clipboard> = None;
                for (content, reply) in rx {
                    let result = match clipboard.as_mut() {
                        Some(cb) => cb.set_text(content),
                        None => arboard::Clipboard::new().and_then(|mut cb| {
                            let r = cb.set_text(content);
                            clipboard = Some(cb);
                            r
                        }),
                    }
                    .map_err(ClipboardError::backend);
                    reply.send(result).ok();
                }
            })
            .ok();
        // If spawning failed, rx was dropped and every copy reports `gone()`.
        tx
    }
}
