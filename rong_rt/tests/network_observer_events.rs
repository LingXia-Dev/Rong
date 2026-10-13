//! Builds every observer event from outside the crate, as an embedder's
//! unit tests would.
#![cfg(feature = "network-observer")]

use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode, Uri, Version};
use rong_rt::http::{
    NetworkBodyChunk, NetworkEvent, NetworkObserver, NetworkOutcome, NetworkOwner,
    NetworkRequestEnd, NetworkRequestId, NetworkRequestKind, NetworkRequestStart,
    NetworkRequestTag, NetworkResponseHead,
};
use std::sync::Mutex;
use std::time::Duration;

#[derive(Default)]
struct Collect(Mutex<Vec<NetworkEvent>>);

impl NetworkObserver for Collect {
    fn on_event(&self, event: NetworkEvent) {
        self.0.lock().unwrap().push(event);
    }
}

#[test]
fn synthetic_events_round_trip_through_an_observer() {
    let id = NetworkRequestId::from_u64(7);
    let uri: Uri = "https://example.com/data".parse().unwrap();
    let mut start =
        NetworkRequestStart::new(id, NetworkRequestKind::Fetch, Method::POST, uri.clone());
    start.owner = Some(NetworkOwner::new("app"));
    start.redirected_from = Some(NetworkRequestId::from_u64(6));
    start.body_size = Some(3);
    let t0 = start.started_at;

    let mut headers = HeaderMap::new();
    headers.insert(http::header::CONTENT_LENGTH, "2".parse().unwrap());
    let events = vec![
        NetworkEvent::RequestStart(start),
        NetworkEvent::RequestBodyChunk(NetworkBodyChunk::new(
            id,
            Bytes::from_static(b"abc"),
            t0 + Duration::from_millis(1),
        )),
        NetworkEvent::ResponseHead(NetworkResponseHead::new(
            id,
            StatusCode::CREATED,
            Version::HTTP_2,
            headers,
            Duration::from_millis(5),
        )),
        NetworkEvent::ResponseBodyChunk(NetworkBodyChunk::new(
            id,
            Bytes::from_static(b"ok"),
            t0 + Duration::from_millis(6),
        )),
        NetworkEvent::RequestEnd(NetworkRequestEnd::new(
            id,
            NetworkOutcome::Failed("reset".into()),
            2,
            Duration::from_millis(9),
        )),
    ];

    let observer = Collect::default();
    for event in events {
        observer.on_event(event);
    }
    let seen = observer.0.into_inner().unwrap();
    assert_eq!(seen.len(), 5);
    assert!(seen.iter().all(|e| e.id() == id));

    let NetworkEvent::RequestStart(start) = &seen[0] else {
        panic!("{seen:?}")
    };
    assert_eq!(start.kind, NetworkRequestKind::Fetch);
    assert_eq!(start.method, Method::POST);
    assert_eq!(start.uri, uri);
    assert_eq!(start.owner.as_ref().unwrap().as_str(), "app");
    assert_eq!(start.redirected_from.unwrap().as_u64(), 6);
    assert_eq!(start.body_size, Some(3));
    assert!(start.headers.is_empty());

    let NetworkEvent::RequestBodyChunk(chunk) = &seen[1] else {
        panic!("{seen:?}")
    };
    assert_eq!(chunk.chunk, "abc");
    assert_eq!(chunk.at, t0 + Duration::from_millis(1));

    let NetworkEvent::ResponseHead(head) = &seen[2] else {
        panic!("{seen:?}")
    };
    assert_eq!(head.status, StatusCode::CREATED);
    assert_eq!(head.version, Version::HTTP_2);
    assert_eq!(head.headers[http::header::CONTENT_LENGTH], "2");
    assert_eq!(head.time_to_first_byte, Duration::from_millis(5));

    let NetworkEvent::ResponseBodyChunk(chunk) = &seen[3] else {
        panic!("{seen:?}")
    };
    assert_eq!(chunk.chunk, "ok");
    assert_eq!(chunk.at, t0 + Duration::from_millis(6));

    let NetworkEvent::RequestEnd(end) = &seen[4] else {
        panic!("{seen:?}")
    };
    assert_eq!(end.outcome, NetworkOutcome::Failed("reset".into()));
    assert_eq!(end.response_bytes, 2);
    assert_eq!(end.duration, Duration::from_millis(9));
}

#[test]
fn tag_insert_into_sets_kind_on_a_plain_request() {
    let mut request = http::Request::new(());
    let tag = NetworkRequestTag::new(NetworkRequestKind::Download);
    let id = tag.id;
    tag.insert_into(&mut request);
    let stored = request.extensions().get::<NetworkRequestTag>().unwrap();
    assert_eq!(stored.id, id);
    assert_eq!(stored.kind, NetworkRequestKind::Download);
}
