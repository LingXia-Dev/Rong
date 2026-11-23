//! Promise rejections nothing handles reach the embedder's tracker once the
//! job queue has drained; on an engine that cannot report an event, asking for
//! it is refused.

use rong_test::*;
use std::cell::RefCell;
use std::rc::Rc;

type Log = Rc<RefCell<Vec<String>>>;

fn text(value: &JSValue) -> String {
    value
        .to_string_coerced()
        .unwrap_or_else(|_| "<unprintable>".to_string())
}

fn line(event: &RejectionEvent) -> String {
    match event {
        RejectionEvent::Unhandled { reason, .. } => format!("unhandled: {}", text(reason)),
        RejectionEvent::HandledLater { reason, .. } => format!("handled later: {}", text(reason)),
    }
}

/// Logs every event, or returns `None` when the engine cannot report `events`
/// after checking that asking for them is refused.
fn track(ctx: &JSContext, events: RejectionEvents) -> JSResult<Option<Log>> {
    let supported = ctx.supports_rejection_tracking();
    let log: Log = Rc::default();
    let sink = log.clone();
    let result = ctx.set_rejection_tracker(events, move |_, event| {
        sink.borrow_mut().push(line(&event));
        Ok(())
    });
    let can = (supported.unhandled || !events.unhandled)
        && (supported.handled_later || !events.handled_later);
    if can {
        result?;
        Ok(Some(log))
    } else {
        let err = result.expect_err("an event the engine cannot report must be refused");
        assert!(err.is_code(rong::error::E_NOT_SUPPORTED), "{err}");
        Ok(None)
    }
}

fn eval(ctx: &JSContext, code: &str) -> JSResult<()> {
    ctx.eval::<()>(Source::from_bytes(code))
}

fn logged(log: &Log) -> Vec<String> {
    log.borrow().clone()
}

#[test]
fn an_unhandled_rejection_is_reported_once() -> JSResult<()> {
    let rt = RongJS::runtime();
    let ctx = rt.context();
    let Some(log) = track(&ctx, RejectionEvents::UNHANDLED)? else {
        return Ok(());
    };
    eval(&ctx, "Promise.reject(new Error('lost')); undefined")?;
    eval(
        &ctx,
        "(async () => { throw new TypeError('in async'); })(); undefined",
    )?;
    rt.run_pending_jobs();
    rt.run_pending_jobs();
    assert_eq!(
        logged(&log),
        ["unhandled: Error: lost", "unhandled: TypeError: in async"]
    );
    ctx.clear_rejection_tracker();
    Ok(())
}

#[test]
fn a_rejection_handled_in_the_same_turn_is_not_reported() -> JSResult<()> {
    let rt = RongJS::runtime();
    let ctx = rt.context();
    let Some(log) = track(&ctx, RejectionEvents::UNHANDLED)? else {
        return Ok(());
    };
    eval(
        &ctx,
        r#"
        const p = Promise.reject(1);
        p.catch(() => {});
        Promise.reject(2).then(null, () => {});
        const q = Promise.reject(3);
        Promise.resolve().then(() => q.catch(() => {}));
        undefined
        "#,
    )?;
    rt.run_pending_jobs();
    rt.run_pending_jobs();
    assert_eq!(logged(&log), Vec::<String>::new());
    ctx.clear_rejection_tracker();
    Ok(())
}

#[test]
fn a_rejection_handled_later_is_reported_again() -> JSResult<()> {
    let rt = RongJS::runtime();
    let ctx = rt.context();
    let Some(log) = track(&ctx, RejectionEvents::ALL)? else {
        return Ok(());
    };
    eval(
        &ctx,
        "globalThis.late = Promise.reject(new Error('late')); undefined",
    )?;
    rt.run_pending_jobs();
    assert_eq!(logged(&log), ["unhandled: Error: late"]);

    eval(&ctx, "late.catch(() => {}); undefined")?;
    rt.run_pending_jobs();
    rt.run_pending_jobs();
    assert_eq!(
        logged(&log),
        ["unhandled: Error: late", "handled later: Error: late"]
    );
    ctx.clear_rejection_tracker();
    Ok(())
}

#[test]
fn rejections_with_any_reason_are_reported_in_order() -> JSResult<()> {
    let rt = RongJS::runtime();
    let ctx = rt.context();
    let Some(log) = track(&ctx, RejectionEvents::UNHANDLED)? else {
        return Ok(());
    };
    eval(
        &ctx,
        r#"
        Promise.reject('boom');
        Promise.reject(undefined);
        Promise.reject(null);
        Promise.reject({ tag: 'plain' });
        for (let i = 0; i < 5; i++) Promise.reject(i);
        undefined
        "#,
    )?;
    rt.run_pending_jobs();
    assert_eq!(
        logged(&log),
        [
            "unhandled: boom",
            "unhandled: undefined",
            "unhandled: null",
            "unhandled: [object Object]",
            "unhandled: 0",
            "unhandled: 1",
            "unhandled: 2",
            "unhandled: 3",
            "unhandled: 4",
        ]
    );
    ctx.clear_rejection_tracker();
    Ok(())
}

/// A rejection the tracker causes is reported after the next drain, so a
/// tracker that always rejects again cannot keep one drain going; a tracker
/// that fails does not stop the events after it.
#[test]
fn a_tracker_that_rejects_or_fails() -> JSResult<()> {
    let rt = RongJS::runtime();
    let ctx = rt.context();
    if !ctx.supports_rejection_tracking().unhandled {
        assert!(track(&ctx, RejectionEvents::UNHANDLED)?.is_none());
        return Ok(());
    }
    let log: Log = Rc::default();
    let sink = log.clone();
    ctx.set_rejection_tracker(RejectionEvents::UNHANDLED, move |ctx, event| {
        let line = line(&event);
        sink.borrow_mut().push(line.clone());
        ctx.eval::<()>(Source::from_bytes(
            "Promise.reject('from tracker'); undefined",
        ))?;
        if line.ends_with("fails") {
            return Err(HostError::new(rong::error::E_INTERNAL, "tracker failed").into());
        }
        Ok(())
    })?;

    eval(
        &ctx,
        "Promise.reject('fails'); Promise.reject('after'); undefined",
    )?;
    rt.run_pending_jobs();
    assert_eq!(logged(&log), ["unhandled: fails", "unhandled: after"]);

    rt.run_pending_jobs();
    rt.run_pending_jobs();
    assert_eq!(
        logged(&log),
        [
            "unhandled: fails",
            "unhandled: after",
            // Both events of the first drain rejected once each.
            "unhandled: from tracker",
            "unhandled: from tracker",
            // Which rejected once more at the second drain.
            "unhandled: from tracker",
            "unhandled: from tracker",
        ]
    );
    ctx.clear_rejection_tracker();
    Ok(())
}

/// The reason is handed over as it is, even when it was made in another
/// context of the runtime.
#[test]
fn a_reason_from_another_context_keeps_its_identity() -> JSResult<()> {
    let rt = RongJS::runtime();
    let b = rt.context();
    let a = rt.context();
    if !a.supports_rejection_tracking().unhandled {
        assert!(track(&a, RejectionEvents::UNHANDLED)?.is_none());
        return Ok(());
    }
    b.eval::<()>(Source::from_bytes(
        "globalThis.failure = new Error('from b');",
    ))?;
    a.global()
        .set("failure", b.global().get::<_, JSObject>("failure")?)?;
    a.set_rejection_tracker(RejectionEvents::UNHANDLED, |ctx, event| {
        if let RejectionEvent::Unhandled { reason, promise } = event {
            ctx.global().set("seenReason", reason)?;
            ctx.global().set("seenPromise", promise)?;
        }
        Ok(())
    })?;
    eval(
        &a,
        "globalThis.rejected = Promise.reject(failure); undefined",
    )?;
    rt.run_pending_jobs();
    let same: bool = a.eval(Source::from_bytes(
        "seenReason === failure && seenPromise === rejected",
    ))?;
    assert!(same);
    a.clear_rejection_tracker();
    Ok(())
}

#[test]
fn a_cleared_tracker_reports_nothing() -> JSResult<()> {
    let rt = RongJS::runtime();
    let ctx = rt.context();
    let Some(log) = track(&ctx, RejectionEvents::UNHANDLED)? else {
        return Ok(());
    };
    eval(&ctx, "Promise.reject('first'); undefined")?;
    ctx.clear_rejection_tracker();
    rt.run_pending_jobs();
    eval(&ctx, "Promise.reject('second'); undefined")?;
    rt.run_pending_jobs();
    assert_eq!(logged(&log), Vec::<String>::new());
    Ok(())
}

/// Pins what each engine reports, so a backend that silently loses its hook
/// fails here instead of skipping the tests above.
#[test]
fn each_engine_reports_what_it_can() {
    let rt = RongJS::runtime();
    let ctx = rt.context();
    let jscore = cfg!(any(
        feature = "primary-jscore",
        all(
            feature = "jscore",
            not(feature = "primary-quickjs"),
            not(feature = "primary-arkjs")
        )
    ));
    let arkjs = !jscore
        && cfg!(any(
            feature = "primary-arkjs",
            all(feature = "arkjs", not(feature = "primary-quickjs"))
        ));
    let expected = if jscore && cfg!(feature = "jscore-source") {
        RejectionEvents::UNHANDLED
    } else if jscore || arkjs {
        // JavaScriptCore's system framework has it only as private API; ArkJS
        // does not route its reject handler here yet.
        RejectionEvents::NONE
    } else {
        RejectionEvents::ALL
    };
    assert_eq!(ctx.supports_rejection_tracking(), expected);
}

const HANDLED_LATER: RejectionEvents = RejectionEvents {
    unhandled: false,
    handled_later: true,
};

#[test]
fn a_tracker_gets_only_the_events_it_asked_for() -> JSResult<()> {
    let rt = RongJS::runtime();
    let ctx = rt.context();
    let Some(log) = track(&ctx, HANDLED_LATER)? else {
        return Ok(());
    };
    eval(
        &ctx,
        r#"
        globalThis.late = Promise.reject(new Error('late'));
        Promise.reject(new Error('in turn')).catch(() => {});
        undefined
        "#,
    )?;
    rt.run_pending_jobs();
    assert!(logged(&log).is_empty(), "{:?}", logged(&log));

    eval(&ctx, "late.catch(() => {}); undefined")?;
    rt.run_pending_jobs();
    assert_eq!(logged(&log), ["handled later: Error: late"]);
    ctx.clear_rejection_tracker();
    Ok(())
}

#[test]
fn a_tracker_for_unhandled_only_is_not_told_of_later_handlers() -> JSResult<()> {
    let rt = RongJS::runtime();
    let ctx = rt.context();
    let Some(log) = track(&ctx, RejectionEvents::UNHANDLED)? else {
        return Ok(());
    };
    eval(
        &ctx,
        "globalThis.late = Promise.reject(new Error('late')); undefined",
    )?;
    rt.run_pending_jobs();
    eval(&ctx, "late.catch(() => {}); undefined")?;
    rt.run_pending_jobs();
    assert_eq!(logged(&log), ["unhandled: Error: late"]);
    ctx.clear_rejection_tracker();
    Ok(())
}
