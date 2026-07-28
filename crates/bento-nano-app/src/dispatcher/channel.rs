use super::*;

// -----------------------------------------------------------------------------
// MPSC dispatcher (kept binary-compatible with Phase 2.1 consumers).
// -----------------------------------------------------------------------------

/// Type-aliased `Sender<Command>` — exposed for producers that want a bare
/// crossbeam handle (background workers wired up via `EventDispatcher::sender`
/// already get a `Sender<Command>`; this alias is for crates that import the
/// type directly).
pub type CommandSender = Sender<Command>;

/// Type-aliased `Receiver<Command>` — symmetric with [`CommandSender`].
pub type CommandReceiver = Receiver<Command>;

/// Hand-rolled (no thiserror — §8.1) error returned from dispatcher send /
/// recv operations when the channel partner has been dropped. Phase 1
/// callers that use the bus rarely care about this case (the receiver is
/// held by the wndproc for the life of the process); it exists so the
/// dispatcher's public surface composes into spec §11's `Result`-only
/// no-panic discipline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatcherError {
    /// The matched receiver has been dropped — the message will never
    /// reach a consumer.
    ReceiverDisconnected,
    /// The matched sender has been dropped — no further messages will
    /// arrive on the receiver.
    SenderDisconnected,
}

impl fmt::Display for DispatcherError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReceiverDisconnected => f.write_str("dispatcher receiver disconnected"),
            Self::SenderDisconnected => f.write_str("dispatcher sender disconnected"),
        }
    }
}

impl core::error::Error for DispatcherError {}

/// MPSC dispatcher. Multiple producers via cloned [`CommandSender`]s; single
/// consumer drains via [`EventDispatcher::drain_into`].
#[derive(Debug, Clone)]
pub struct EventDispatcher {
    tx: CommandSender,
    rx: CommandReceiver,
}

impl Default for EventDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl EventDispatcher {
    pub fn new() -> Self {
        let (tx, rx) = unbounded();
        Self { tx, rx }
    }

    /// Get a clonable producer handle for use on background threads.
    pub fn sender(&self) -> CommandSender {
        self.tx.clone()
    }

    /// Push a command synchronously from the UI thread. Returns `false`
    /// when the receiver has been dropped (Phase-1 contract: this only
    /// happens during shutdown after WM_DESTROY).
    pub fn push(&self, cmd: Command) -> bool {
        self.tx.send(cmd).is_ok()
    }

    /// Drain all pending commands into `out`. Returns the number drained.
    pub fn drain_into(&self, out: &mut smallvec::SmallVec<[Command; 8]>) -> usize {
        let mut count = 0;
        while let Ok(c) = self.rx.try_recv() {
            out.push(c);
            count += 1;
        }
        count
    }
}

// -----------------------------------------------------------------------------
// T-014 — Request/reply channel for backend ops (LoadIcon, etc.)
// -----------------------------------------------------------------------------

/// One pending request — pairs a `req` with the one-shot reply channel the
/// caller blocks on. The backend handler `recv()`s the request, computes
/// the response, and `send()`s it back through `reply`.
///
/// Single-thread invariant (spec §9): the backend handler runs on its own
/// `std::thread` worker (T-100 future task) consuming `Request<Req, Resp>`;
/// the UI thread sends the request and `recv()`s on the bounded(1) reply
/// channel. NO tokio anywhere in this path — crossbeam-channel is the
/// only inter-thread mechanism.
#[derive(Debug)]
pub struct Request<Req, Resp> {
    pub req: Req,
    pub reply: Sender<Resp>,
}

/// Producer half of the request/reply channel.
#[derive(Debug, Clone)]
pub struct RequestSender<Req, Resp> {
    tx: Sender<Request<Req, Resp>>,
}

impl<Req, Resp> RequestSender<Req, Resp> {
    /// Send a request. Returns `Err(DispatcherError::ReceiverDisconnected)`
    /// when the backend worker has shut down.
    pub fn send(&self, request: Request<Req, Resp>) -> Result<(), DispatcherError> {
        self.tx
            .send(request)
            .map_err(|_| DispatcherError::ReceiverDisconnected)
    }
}

/// Consumer half of the request/reply channel — owned by the backend worker.
#[derive(Debug)]
pub struct RequestReceiver<Req, Resp> {
    rx: Receiver<Request<Req, Resp>>,
}

impl<Req, Resp> RequestReceiver<Req, Resp> {
    /// Block until a request arrives. Returns
    /// `Err(DispatcherError::SenderDisconnected)` when every sender has
    /// been dropped.
    pub fn recv(&self) -> Result<Request<Req, Resp>, DispatcherError> {
        self.rx
            .recv()
            .map_err(|_| DispatcherError::SenderDisconnected)
    }

    /// Non-blocking variant. `Ok(None)` when the channel is empty;
    /// `Err(SenderDisconnected)` when every sender has been dropped.
    pub fn try_recv(&self) -> Result<Option<Request<Req, Resp>>, DispatcherError> {
        match self.rx.try_recv() {
            Ok(r) => Ok(Some(r)),
            Err(crossbeam_channel::TryRecvError::Empty) => Ok(None),
            Err(crossbeam_channel::TryRecvError::Disconnected) => {
                Err(DispatcherError::SenderDisconnected)
            }
        }
    }
}

/// Construct a request/reply channel pair with capacity `cap` on the
/// request queue. `cap == 0` is rejected (returns `Err`) because crossbeam
/// rendezvous channels would deadlock the request side; Phase 1 callers
/// always pick `cap >= 1`.
///
/// Caller pattern (UI thread, Phase 4 IconPicker example):
/// ```ignore
/// let (resp_tx, resp_rx) = crossbeam_channel::bounded(1);
/// request_sender.send(Request { req: IconRequest { path }, reply: resp_tx })?;
/// let icon_hash = resp_rx.recv()?; // blocks until backend replies
/// ```
///
/// Backend pattern (worker thread, T-100 future):
/// ```ignore
/// while let Ok(req) = receiver.recv() {
///     let resp = handle_load_icon(&req.req);
///     let _ = req.reply.send(resp); // ignore disconnect
/// }
/// ```
/// Pair returned by [`request_channel`] — kept as a named alias so the
/// signature stays under clippy's `type_complexity` threshold.
pub type RequestPair<Req, Resp> = (RequestSender<Req, Resp>, RequestReceiver<Req, Resp>);

pub fn request_channel<Req, Resp>(cap: usize) -> Result<RequestPair<Req, Resp>, DispatcherError> {
    if cap == 0 {
        // Rendezvous would force the request side to block until the
        // backend `recv()`s — Phase 1 expects fire-and-block-on-reply
        // semantics, not back-pressure on the send side. Fall back to a
        // 1-slot bounded channel internally is a possibility, but spec
        // §11 says surface the misuse explicitly so callers fix it.
        return Err(DispatcherError::ReceiverDisconnected);
    }
    let (tx, rx) = bounded(cap);
    Ok((RequestSender { tx }, RequestReceiver { rx }))
}
