# rong_worker

Worker APIs for the Rong JavaScript runtime.

This crate provides worker-oriented JavaScript bindings for Rong, including the
runtime pieces needed to create and coordinate long-lived worker execution from
JavaScript.

Enable the matching engine feature (`quickjs` or `jscore`) when depending on
this crate directly.

`Worker.terminate()` stops non-yielding JavaScript on engines that report
preemptive interruption; cooperative-only engines can stop running code at its
next engine yield. The call always returns immediately. A bounded background
reaper joins stopped threads without occupying the host blocking pool
indefinitely; hosts can inspect `termination_stats()` for worker threads that
had to be detached after the grace period.
