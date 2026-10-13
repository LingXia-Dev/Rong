//! Per-context network owner for `fetch` and `Rong.SSE`.

#[cfg(feature = "network-observer")]
mod enabled {
    use rong::JSContext;
    use rong_rt::http::{
        NetworkOwner, NetworkRequestId, NetworkRequestKind, NetworkRequestTag,
        current_network_owner, network_observer_installed, notify_network_redirect,
        sync_scope_network_owner,
    };

    struct ContextNetworkOwner(Option<NetworkOwner>);

    /// Attribute `fetch` and `Rong.SSE` traffic from `ctx` to `owner`.
    ///
    /// Captured when each request starts, so it holds across the hop to the
    /// runtime's I/O thread and across redirects. `None` clears it.
    pub fn set_network_owner(ctx: &JSContext, owner: Option<NetworkOwner>) {
        ctx.set_state(ContextNetworkOwner(owner));
    }

    /// The owner set on `ctx`, falling back to the enclosing
    /// `rong_rt::http::scope_network_owner`.
    pub fn network_owner(ctx: &JSContext) -> Option<NetworkOwner> {
        ctx.get_state::<ContextNetworkOwner>()
            .and_then(|state| state.0.clone())
            .or_else(current_network_owner)
    }

    pub(crate) struct FetchOwner {
        owner: Option<NetworkOwner>,
        hop: Option<NetworkRequestId>,
    }

    impl FetchOwner {
        pub(crate) fn new(ctx: &JSContext) -> Self {
            Self {
                owner: network_owner(ctx),
                hop: None,
            }
        }

        pub(crate) fn tag<B>(&mut self, request: &mut http::Request<B>) {
            if !network_observer_installed() {
                self.hop = None;
                return;
            }
            let tag = NetworkRequestTag::new(NetworkRequestKind::Fetch)
                .with_owner(self.owner.clone())
                .with_redirected_from(self.hop);
            self.hop = Some(tag.id);
            request.extensions_mut().insert(tag);
        }

        pub(crate) fn redirect(&self, status: http::StatusCode, from: &http::Uri, to: &http::Uri) {
            if let Some(id) = self.hop {
                notify_network_redirect(id, status, from, to);
            }
        }
    }

    pub(crate) fn with_context_owner<T>(ctx: &JSContext, f: impl FnOnce() -> T) -> T {
        match network_owner(ctx) {
            Some(owner) => sync_scope_network_owner(owner, f),
            None => f(),
        }
    }
}

#[cfg(feature = "network-observer")]
pub use enabled::{network_owner, set_network_owner};

#[cfg(feature = "network-observer")]
pub(crate) use enabled::{FetchOwner, with_context_owner};

#[cfg(not(feature = "network-observer"))]
mod disabled {
    use rong::JSContext;

    pub(crate) struct FetchOwner;

    impl FetchOwner {
        #[inline(always)]
        pub(crate) fn new(_ctx: &JSContext) -> Self {
            Self
        }
        #[inline(always)]
        pub(crate) fn tag<B>(&mut self, _request: &mut http::Request<B>) {}
        #[inline(always)]
        pub(crate) fn redirect(
            &self,
            _status: http::StatusCode,
            _from: &http::Uri,
            _to: &http::Uri,
        ) {
        }
    }

    #[inline(always)]
    pub(crate) fn with_context_owner<T>(_ctx: &JSContext, f: impl FnOnce() -> T) -> T {
        f()
    }
}

#[cfg(not(feature = "network-observer"))]
pub(crate) use disabled::{FetchOwner, with_context_owner};
