//! One automatic desktop offer per workspace lifetime; never an expiry timer.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Request {
    pub revision: u64,
    pub open: bool,
}
impl Request {
    pub(super) fn show(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.open = true;
    }
    pub(super) fn close(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.open = false;
    }
}

#[derive(Default)]
pub(super) struct Offer {
    attempted: bool,
    consumed: u64,
}
impl Offer {
    /// Some(true): explicitly/automatically open; Some(false): explicitly close.
    /// None means no action, including after expiry, failure and reconnect.
    pub(super) fn next(&mut self, request: Request, automatic_ready: bool) -> Option<bool> {
        if request.revision != self.consumed {
            self.consumed = request.revision;
            self.attempted = true;
            return Some(request.open);
        }
        if automatic_ready && !self.attempted {
            self.attempted = true;
            return Some(true);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn automatic_offer_waits_for_readiness_and_never_reissues_itself() {
        let mut offer = Offer::default();
        let request = Request::default();
        assert_eq!(offer.next(request, false), None);
        assert_eq!(offer.next(request, true), Some(true));
        // Expiry, dismissal, failure, page/layout change and reconnect do not
        // mutate the request, and must not mint another bearer credential.
        for ready in [true, false, true, false, true] {
            assert_eq!(offer.next(request, ready), None);
        }
    }
    #[test]
    fn only_a_new_explicit_request_can_open_again_and_close_cancels_auto() {
        let mut offer = Offer::default();
        let mut request = Request::default();
        request.close();
        assert_eq!(offer.next(request, true), Some(false));
        assert_eq!(offer.next(request, true), None);
        request.show();
        assert_eq!(offer.next(request, true), Some(true));
        assert_eq!(offer.next(request, true), None);
        request.show();
        assert_eq!(offer.next(request, true), Some(true));
    }
}
