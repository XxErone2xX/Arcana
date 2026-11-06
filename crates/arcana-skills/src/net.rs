//! 인터넷 연결 (위키백과, 웹사이트, 검색, 업데이트 확인 전부 여기를 거침)
//!
//! - 보안 연결은 윈도우 인증서 저장소를 씀 (native-tls)
//!   → 백신·보안 프로그램이 HTTPS를 검사하는 PC에서도 브라우저처럼 연결됨
//! - 실패하면 내장 인증서(rustls)로 한 번 더 시도
//! - 윈도우 프록시 설정(인터넷 옵션)과 HTTPS_PROXY 환경 변수를 따름
//! - 실패하면 원인을 한국어로 설명 (/Diag 로 자세히 확인)

use std::sync::{Arc, OnceLock};
use std::time::Duration;

pub const USER_AGENT: &str = concat!(
    "ArcanaAssistant/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/XxErone2xX/Arcana)"
);

struct Agents {
    native: Option<ureq::Agent>,
    rustls: ureq::Agent,
    proxy: Option<String>,
}

fn base(proxy: &Option<String>) -> ureq::AgentBuilder {
    let mut b = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(8))
        .timeout_read(Duration::from_secs(20))
        .user_agent(USER_AGENT);
    if let Some(p) = proxy.as_ref().and_then(|p| ureq::Proxy::new(p).ok()) {
        b = b.proxy(p);
    }
    b
}

fn agents() -> &'static Agents {
    static A: OnceLock<Agents> = OnceLock::new();
    A.get_or_init(|| {
        let proxy = system_proxy();
        let native = native_tls::TlsConnector::new()
            .ok()
            .map(|c| base(&proxy).tls_connector(Arc::new(c)).build());
        Agents {
            native,
            rustls: base(&proxy).build(),
            proxy,
        }
    })
}

/// 윈도우 프록시 설정 → "http://호스트:포트"
fn system_proxy() -> Option<String> {
    for k in ["HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy"] {
        if let Ok(v) = std::env::var(k) {
            if !v.trim().is_empty() {
                return Some(v.trim().to_string());
            }
        }
    }
    #[cfg(windows)]
    {
        let key = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings";
        let query = |name: &str| -> Option<String> {
            let out = crate::util::hidden_command("reg")
                .args(["query", key, "/v", name])
                .output()
                .ok()?;
            let s = String::from_utf8_lossy(&out.stdout).to_string();
            s.lines()
                .find(|l| l.contains(name))
                .and_then(|l| l.split_whitespace().last())
                .map(String::from)
        };
        if query("ProxyEnable").as_deref() == Some("0x1") {
            if let Some(server) = query("ProxyServer") {
                // "host:port" 또는 "http=host:port;https=host:port"
                let pick = server
                    .split(';')
                    .find_map(|p| p.strip_prefix("https="))
                    .or_else(|| server.split(';').find_map(|p| p.strip_prefix("http=")))
                    .unwrap_or(&server)
                    .to_string();
                if !pick.contains('=') {
                    return Some(if pick.contains("://") {
                        pick
                    } else {
                        format!("http://{pick}")
                    });
                }
            }
        }
    }
    None
}

/// 실패 이유를 한국어로
pub fn describe(e: &ureq::Error) -> String {
    match e {
        ureq::Error::Status(code, _) => match code {
            403 => "사이트가 접근을 막았습니다 (403)".into(),
            404 => "페이지가 없습니다 (404)".into(),
            429 => "요청이 너무 많아 사이트가 잠시 막았습니다 (429)".into(),
            c if *c >= 500 => format!("사이트 서버 오류입니다 ({c})"),
            c => format!("사이트 응답 오류입니다 ({c})"),
        },
        ureq::Error::Transport(t) => {
            let msg = t.to_string();
            let low = msg.to_lowercase();
            let why = if low.contains("certificate")
                || low.contains("unknownissuer")
                || low.contains("tls")
                || low.contains("handshake")
            {
                "보안 연결(TLS)에 실패했습니다. 백신·보안 프로그램의 HTTPS 검사 때문일 수 있습니다"
            } else if matches!(t.kind(), ureq::ErrorKind::Dns) {
                "주소를 찾지 못했습니다 (DNS). 인터넷 연결이나 DNS 설정을 확인해 주십시오"
            } else if matches!(
                t.kind(),
                ureq::ErrorKind::ProxyConnect
                    | ureq::ErrorKind::ProxyUnauthorized
                    | ureq::ErrorKind::InvalidProxyUrl
            ) {
                "프록시 연결에 실패했습니다"
            } else if low.contains("timed out") || low.contains("timeout") {
                "응답 시간이 초과됐습니다"
            } else if matches!(t.kind(), ureq::ErrorKind::ConnectionFailed) {
                "서버에 연결하지 못했습니다. 방화벽이나 인터넷 연결을 확인해 주십시오"
            } else {
                "연결 중 오류가 발생했습니다"
            };
            let short: String = msg.chars().take(140).collect();
            format!("{why} ({short})")
        }
    }
}

fn is_transport(e: &ureq::Error) -> bool {
    matches!(e, ureq::Error::Transport(_))
}

/// GET 요청 (윈도우 인증서 → 실패 시 내장 인증서로 재시도)
pub fn get(url: &str, headers: &[(&str, &str)]) -> Result<ureq::Response, ureq::Error> {
    let a = agents();
    let call = |agent: &ureq::Agent| {
        let mut r = agent.get(url);
        for (k, v) in headers {
            r = r.set(k, v);
        }
        r.call()
    };
    match &a.native {
        Some(native) => match call(native) {
            Err(e) if is_transport(&e) => call(&a.rustls).map_err(|_| e),
            other => other,
        },
        None => call(&a.rustls),
    }
}

/// 연결 진단 (/Diag): 사이트별로 두 가지 보안 연결 방식을 다 시험
pub fn diagnose() -> Vec<String> {
    let a = agents();
    let mut out = vec![format!(
        "프록시: {}",
        a.proxy.clone().unwrap_or_else(|| "사용 안 함".into())
    )];
    let targets = [
        ("GitHub (업데이트)", "https://api.github.com/repos/XxErone2xX/Arcana"),
        ("위키백과 요약", "https://ko.wikipedia.org/api/rest_v1/page/summary/%EB%B8%94%EB%9E%99%ED%99%80"),
        ("위키백과 검색", "https://ko.wikipedia.org/w/api.php?action=query&list=search&srlimit=1&format=json&srsearch=test"),
        ("DuckDuckGo", "https://api.duckduckgo.com/?format=json&q=rust"),
        ("일반 웹사이트", "https://www.rust-lang.org/"),
    ];
    for (name, url) in targets {
        let t = |agent: &ureq::Agent| match agent.get(url).call() {
            Ok(r) => format!("성공 ({})", r.status()),
            Err(e) => format!("실패 — {}", describe(&e)),
        };
        let native = a
            .native
            .as_ref()
            .map(t)
            .unwrap_or_else(|| "사용 불가".into());
        let rustls = t(&a.rustls);
        out.push(format!(
            "{name}\n  윈도우 인증서: {native}\n  내장 인증서: {rustls}"
        ));
    }
    out
}
