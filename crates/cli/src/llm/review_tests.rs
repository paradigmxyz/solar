use super::*;
use snapbox::IntoData;
use solar_config::{CompileOpts, UnstableOpts};
use solar_interface::ColorChoice;

struct Denied;

impl ChatTransport for Denied {
    fn send(
        &self,
        _: reqwest::Request,
    ) -> Pin<
        Box<
            dyn Future<Output = std::result::Result<reqwest::Response, TransportError>> + Send + '_,
        >,
    > {
        Box::pin(async { Err(TransportError::permanent("test transport denied the request")) })
    }
}

fn setup(model: &str) -> (Session, SessionBinding<dyn ChatTransport>) {
    let opts = CompileOpts {
        threads: 1.into(),
        unstable: UnstableOpts {
            llm_optimize: Some(LlmOptimizeMode::Live),
            llm_model: Some(model.into()),
            ..Default::default()
        },
        ..Default::default()
    };
    let session = Session::builder().opts(opts).with_buffer_emitter(ColorChoice::Never).build();
    let binding = bind_transport(&session, Arc::new(Denied));
    (session, binding)
}

#[tokio::test]
async fn invalid_model_does_not_drop_a_runtime_in_async_code() {
    let (session, _binding) = setup("anthropic/");
    assert!(install(&session).is_err());
}

#[tokio::test]
async fn invalid_endpoint_drops_its_runtime_safely() {
    let (mut session, _binding) = setup("anthropic/test-model");
    session.opts.unstable.llm_endpoint = Some("invalid endpoint".into());
    assert!(install(&session).is_err());
}

#[tokio::test]
async fn finishing_live_rewriting_is_safe_in_async_code() {
    let (session, _binding) = setup("anthropic/test-model");
    install(&session).unwrap().unwrap().finish(&session);
}

#[test]
fn budget_counters_saturate() {
    let (session, _binding) = setup("anthropic/test-model");
    let installed = install(&session).unwrap().unwrap();
    installed.shared.tokens.store(u64::MAX - 1, Ordering::Relaxed);
    installed.shared.spent_nano_usd.store(u64::MAX - 1, Ordering::Relaxed);
    installed.shared.record(10, Some(10));
    assert_eq!(installed.shared.tokens.load(Ordering::Relaxed), u64::MAX);
    assert_eq!(installed.shared.spent_nano_usd.load(Ordering::Relaxed), u64::MAX);
    assert!(!installed.shared.affordable());
    installed.finish(&session);
}

pub(super) fn chat(installed: &Installed) -> HttpChat {
    match &installed.shared.backend {
        Backend::Chat { client, model, effort } => HttpChat {
            client: Arc::clone(client),
            transcript: Transcript::new(client.protocol(), model.clone(), *effort),
        },
    }
}

#[test]
fn failed_turn_closes_admission_for_other_functions() {
    let (session, _binding) = setup("anthropic/test-model");
    let installed = install(&session).unwrap().unwrap();
    let mut chat = chat(&installed);
    let voice = Arc::new(Voice::new(false, "C", "f"));
    for _ in 0..20 {
        assert!(chat.turn(&installed.shared, "prompt".into(), &voice).is_err());
    }
    assert_eq!(installed.shared.asked.load(Ordering::Relaxed), 1);
    assert_eq!(installed.shared.unpriced.load(Ordering::Relaxed), 1);
    assert!(!installed.shared.affordable());
    installed.finish(&session);
}

#[test]
fn cancelled_turn_is_unaccounted_and_closes_the_budget() {
    let (session, _binding) = setup("anthropic/test-model");
    let installed = install(&session).unwrap().unwrap();
    let shared = Arc::clone(&installed.shared);
    let future = async move {
        let _pending = PendingTurn::new(shared).unwrap();
        std::future::pending::<()>().await;
    };
    assert!(wait(installed.runtime.handle(), future, Duration::from_millis(50)).is_err());
    let deadline = Instant::now() + Duration::from_secs(2);
    while installed.shared.affordable() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(installed.shared.asked.load(Ordering::Relaxed), 1);
    assert_eq!(installed.shared.unpriced.load(Ordering::Relaxed), 1);
    installed.finish(&session);
}

#[test]
fn zero_reported_usage_still_has_a_turn_limit() {
    let (session, _binding) = setup("anthropic/test-model");
    let installed = install(&session).unwrap().unwrap();
    for _ in 0..BUDGET_TURNS {
        let mut turn = PendingTurn::new(Arc::clone(&installed.shared)).unwrap();
        turn.complete(0, Some(0));
    }
    assert!(!installed.shared.affordable());
    assert!(PendingTurn::new(Arc::clone(&installed.shared)).is_err());
    installed.finish(&session);
}

#[test]
fn provider_errors_cannot_control_the_trace_terminal() {
    let error = provider_error("bad\u{1b}]52;c;payload\u{7}\nforged\u{202e}".into());
    snapbox::assert_data_eq!(
        error.to_string(),
        snapbox::str![[r#"bad\u{1b}]52;c;payload\u{7}\u{a}forged\u{202e}"#]].raw()
    );
}
