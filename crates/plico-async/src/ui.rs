use super::*;
use serde::{Serialize, de::DeserializeOwned};
use std::{future::poll_fn, rc::Weak};

/// Waiting can finish through the task or a synchronous UI action (e.g. Cancel).
#[derive(Debug, PartialEq)]
pub enum WaitOutcome<T, A> {
    Completed(T),
    Action(A),
}

/// Long-lived scope capability; the actual UiContext is borrowed only while a
/// synchronous frame closure executes. It is deliberately !Send.
#[derive(Clone)]
pub struct AsyncUi {
    pub(crate) shared: Weak<Shared>,
    pub(crate) state: Rc<ScopeState>,
}
impl AsyncUi {
    pub fn id(&self) -> ScopeId {
        self.state.id
    }
    pub fn invalidate(&self) {
        self.state.wake.schedule();
    }
    pub fn state<T: DeserializeOwned>(&self, key: &str, init: impl FnOnce() -> T) -> T {
        let restored = {
            let saved = self.state.saved.borrow();
            saved
                .get(key)
                .and_then(|bytes| rmp_serde::from_slice(bytes).ok())
        };
        restored.unwrap_or_else(init)
    }
    pub fn checkpoint<T: Serialize + ?Sized>(&self, key: &str, value: &T) -> Result<()> {
        let bytes = rmp_serde::to_vec_named(value).map_err(|source| Error::Encode { source })?;
        let mut saved = self.state.saved.borrow_mut();
        if let Some(slot) = saved.get_mut(key) {
            *slot = bytes;
        } else {
            saved.insert(key.to_owned(), bytes);
        }
        Ok(())
    }
    fn available(&self) -> Result<bool> {
        if !self.state.polling.get() {
            return Err(Error::OutsidePoll);
        }
        Ok(self.state.drawn.get() != Some(self.state.pass.get()))
    }
    fn draw<R>(&self, build: impl FnOnce(&mut UiContext<'_>) -> R) -> Result<R> {
        let shared = self.shared.upgrade().ok_or(Error::Closed)?;
        let mut engine = shared
            .engine
            .try_borrow_mut()
            .map_err(|_| Error::Reentrant)?;
        self.state.drawn.set(Some(self.state.pass.get()));
        let mut output = None;
        if shared.external_layout {
            engine.frame_scope_declarations(self.state.id.anchor, |cx| output = Some(build(cx)))?;
        } else {
            engine.frame_scope(self.state.id.anchor, |cx| output = Some(build(cx)))?;
        }
        let changes = engine.changes();
        self.state.animate.set(changes.contains(Changes::ANIMATE));
        if changes.contains(Changes::VIEW) {
            self.invalidate();
        }
        drop(engine);
        // Multiple draws may occur before pump returns, so drain every transaction.
        shared.capture();
        Ok(output.unwrap())
    }
    pub async fn frame<R>(&self, build: impl FnOnce(&mut UiContext<'_>) -> R) -> Result<R> {
        let mut build = Some(build);
        poll_fn(|_| match self.available() {
            Ok(false) => Poll::Pending,
            Ok(true) => Poll::Ready(self.draw(build.take().unwrap())),
            Err(error) => Poll::Ready(Err(error)),
        })
        .await
    }
    /// Draw a live waiting view on host input/environment/animation wakes, and
    /// resume when the host-owned Future resolves. Completion wins a same-poll race.
    pub async fn while_pending<F: Future>(
        &self,
        task: F,
        mut loading: impl FnMut(&mut UiContext<'_>),
    ) -> Result<F::Output> {
        match self
            .while_pending_or(task, |cx| {
                loading(cx);
                None::<std::convert::Infallible>
            })
            .await?
        {
            WaitOutcome::Completed(result) => Ok(result),
            WaitOutcome::Action(never) => match never {},
        }
    }
    /// Like while_pending, but Some(action) from the waiting view drops the
    /// task Future and returns immediately. It does not undo remote side effects.
    pub async fn while_pending_or<F: Future, A>(
        &self,
        task: F,
        mut loading: impl FnMut(&mut UiContext<'_>) -> Option<A>,
    ) -> Result<WaitOutcome<F::Output, A>> {
        if self.state.drawn.get() == Some(self.state.pass.get()) {
            self.invalidate();
        }
        let mut task = std::pin::pin!(task);
        poll_fn(|cx| {
            if let Poll::Ready(result) = task.as_mut().poll(cx) {
                return Poll::Ready(Ok(WaitOutcome::Completed(result)));
            }
            match self.available() {
                Ok(true) => match self.draw(&mut loading) {
                    Ok(Some(action)) => {
                        self.invalidate(); // A next-step frame cannot draw twice in this pass.
                        return Poll::Ready(Ok(WaitOutcome::Action(action)));
                    }
                    Ok(None) => {}
                    Err(error) => return Poll::Ready(Err(error)),
                },
                Ok(false) => {}
                Err(error) => return Poll::Ready(Err(error)),
            }
            Poll::Pending
        })
        .await
    }
    /// Nested async children use the same mount semantics as a synchronous parent.
    pub fn mount<F: Future<Output = Result<()>> + 'static>(
        &self,
        layout: Layout<'_, '_>,
        make: impl FnOnce(AsyncUi) -> F + 'static,
    ) -> Result<ScopeId> {
        let shared = self.shared.upgrade().ok_or(Error::Closed)?;
        if !self.state.polling.get() {
            return Err(Error::OutsidePoll);
        }
        Ok(Mounts { shared: &shared }.mount(layout, make))
    }
}
