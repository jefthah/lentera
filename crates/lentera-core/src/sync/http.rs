//! Klien HTTP untuk server Lentera, dengan `ureq` (sudah ikut di aplikasi
//! lewat Velopack). Semua panggilan memblokir; jalankan di thread latar.

use std::time::Duration;

use serde::{Serialize, de::DeserializeOwned};
use ureq::{
    Agent,
    http::{Response, StatusCode},
};

use super::{
    Api, ApiError, CodeSent, ProfilePut, RemoteAccount, RemoteProfile, SyncRequest, SyncResponse,
    Verified,
};

/// Jawaban terbesar yang dibaca: satu halaman sinkron sekitar 4 MB.
const MAX_RESPONSE_BYTES: u64 = 32 << 20;

/// Server Lentera di `base` (misalnya `https://api.contoh.org`), dengan
/// token sesi bila sudah masuk.
pub struct HttpApi {
    base: String,
    token: Option<String>,
    agent: Agent,
}

impl HttpApi {
    pub fn new(base: &str, token: Option<String>) -> Self {
        let agent: Agent = Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(10)))
            .timeout_global(Some(Duration::from_secs(60)))
            .http_status_as_error(false)
            .user_agent(format!("Lentera/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .into();
        Self {
            base: base.trim_end_matches('/').into(),
            token,
            agent,
        }
    }

    /// Meminta kode masuk 6 digit ke email ini.
    pub fn send_code(&self, email: &str) -> Result<CodeSent, ApiError> {
        #[derive(Serialize)]
        struct Body<'a> {
            email: &'a str,
        }
        self.post("/v1/auth/code", &Body { email })
    }

    /// Menukar kode masuk dengan token sesi untuk perangkat ini.
    pub fn verify(&self, email: &str, code: &str, device: &str) -> Result<Verified, ApiError> {
        #[derive(Serialize)]
        struct Body<'a> {
            email: &'a str,
            code: &'a str,
            device: &'a str,
        }
        self.post(
            "/v1/auth/verify",
            &Body {
                email,
                code,
                device,
            },
        )
    }

    /// Akun pemilik token ini.
    pub fn me(&self) -> Result<RemoteAccount, ApiError> {
        let response = self.with_token(self.agent.get(self.url("/v1/me"))).call();
        read(response)
    }

    /// Mengakhiri sesi perangkat ini di server.
    pub fn logout(&self) -> Result<(), ApiError> {
        let response = self
            .with_token(self.agent.post(self.url("/v1/auth/logout")))
            .send("");
        read_empty(response)
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    fn with_token<B>(&self, request: ureq::RequestBuilder<B>) -> ureq::RequestBuilder<B> {
        match &self.token {
            Some(token) => request.header("Authorization", format!("Bearer {token}")),
            None => request,
        }
    }

    fn post<T: DeserializeOwned>(&self, path: &str, body: &impl Serialize) -> Result<T, ApiError> {
        let body = serde_json::to_string(body).map_err(ApiError::offline)?;
        let response = self
            .with_token(self.agent.post(self.url(path)))
            .content_type("application/json")
            .send(body);
        read(response)
    }
}

impl Api for HttpApi {
    fn profiles(&self) -> Result<Vec<RemoteProfile>, ApiError> {
        #[derive(serde::Deserialize)]
        struct List {
            profiles: Vec<RemoteProfile>,
        }
        let response = self
            .with_token(self.agent.get(self.url("/v1/profiles")))
            .call();
        read::<List>(response).map(|list| list.profiles)
    }

    fn put_profile(&self, id: &str, body: &ProfilePut) -> Result<RemoteProfile, ApiError> {
        let body = serde_json::to_string(body).map_err(ApiError::offline)?;
        let response = self
            .with_token(self.agent.put(self.url(&format!("/v1/profiles/{id}"))))
            .content_type("application/json")
            .send(body);
        read(response)
    }

    fn delete_profile(&self, id: &str) -> Result<(), ApiError> {
        let response = self
            .with_token(self.agent.delete(self.url(&format!("/v1/profiles/{id}"))))
            .call();
        read_empty(response)
    }

    fn sync(&self, id: &str, request: &SyncRequest) -> Result<SyncResponse, ApiError> {
        self.post(&format!("/v1/profiles/{id}/sync"), request)
    }
}

/// Nama perangkat untuk daftar sesi di server, misalnya nama komputer.
pub fn device_name() -> String {
    let name = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_default();
    let name = name.trim();
    let os = if cfg!(windows) {
        "Windows"
    } else if cfg!(target_os = "macos") {
        "Mac"
    } else {
        "Komputer"
    };
    if name.is_empty() {
        format!("Lentera di {os}")
    } else {
        format!("{name} ({os})")
    }
}

fn read<T: DeserializeOwned>(
    response: Result<Response<ureq::Body>, ureq::Error>,
) -> Result<T, ApiError> {
    let (status, text) = body(response)?;
    if !status.is_success() {
        return Err(error(status, &text, None));
    }
    serde_json::from_str(&text).map_err(|e| ApiError {
        status: status.as_u16(),
        code: "bad_response".into(),
        message: format!("Jawaban server tidak dikenal.\n{e}"),
        field: None,
        retry_after: None,
    })
}

fn read_empty(response: Result<Response<ureq::Body>, ureq::Error>) -> Result<(), ApiError> {
    let (status, text) = body(response)?;
    if status.is_success() {
        Ok(())
    } else {
        Err(error(status, &text, None))
    }
}

fn body(
    response: Result<Response<ureq::Body>, ureq::Error>,
) -> Result<(StatusCode, String), ApiError> {
    let response = response.map_err(ApiError::offline)?;
    let status = response.status();
    let retry_after = response
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse().ok());
    let mut body = response.into_body();
    let text = body
        .with_config()
        .limit(MAX_RESPONSE_BYTES)
        .read_to_string()
        .map_err(ApiError::offline)?;
    if status == StatusCode::TOO_MANY_REQUESTS {
        let mut e = error(status, &text, retry_after);
        e.retry_after = retry_after.or(Some(60));
        return Err(e);
    }
    Ok((status, text))
}

fn error(status: StatusCode, text: &str, retry_after: Option<u64>) -> ApiError {
    #[derive(serde::Deserialize)]
    struct Wrapper {
        error: Detail,
    }
    #[derive(serde::Deserialize)]
    struct Detail {
        code: String,
        message: String,
        field: Option<String>,
    }
    match serde_json::from_str::<Wrapper>(text) {
        Ok(Wrapper { error }) => ApiError {
            status: status.as_u16(),
            code: error.code,
            message: error.message,
            field: error.field.filter(|f| !f.is_empty()),
            retry_after,
        },
        Err(_) => ApiError {
            status: status.as_u16(),
            code: String::new(),
            message: format!("Server Lentera menjawab {status}. Coba lagi nanti."),
            field: None,
            retry_after,
        },
    }
}
