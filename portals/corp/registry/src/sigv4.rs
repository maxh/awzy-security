//! AWS Signature Version 4 for the one AWS call the proxy makes (ECR GetAuthorizationToken),
//! so the crate doesn't pull in the AWS SDK.

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone)]
pub struct Credentials {
    pub access_key_id: String,
    pub secret_access_key: String,
    pub session_token: Option<String>,
}

pub struct Request<'a> {
    pub method: &'a str,
    pub host: &'a str,
    pub path: &'a str,
    pub query: &'a str,
    /// Lowercase names. `host` and `x-amz-date` are added here.
    pub headers: &'a [(&'a str, &'a str)],
    pub payload: &'a [u8],
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hmac(key: &[u8], data: &str) -> Vec<u8> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC takes any key length");
    mac.update(data.as_bytes());
    mac.finalize().into_bytes().to_vec()
}

/// The headers to add to `req`: `x-amz-date`, `x-amz-security-token` (with temporary
/// credentials), and `authorization`. `amz_date` is `YYYYMMDDTHHMMSSZ`.
pub fn sign(
    creds: &Credentials,
    region: &str,
    service: &str,
    req: &Request,
    amz_date: &str,
) -> Vec<(&'static str, String)> {
    let date = &amz_date[..8];
    let mut headers: Vec<(String, String)> =
        req.headers.iter().map(|(k, v)| (k.to_string(), v.trim().to_string())).collect();
    headers.push(("host".into(), req.host.into()));
    headers.push(("x-amz-date".into(), amz_date.into()));
    if let Some(token) = &creds.session_token {
        headers.push(("x-amz-security-token".into(), token.clone()));
    }
    headers.sort();
    let canonical_headers: String = headers.iter().map(|(k, v)| format!("{k}:{v}\n")).collect();
    let signed_headers = headers.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>().join(";");
    let canonical_request = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        req.method,
        req.path,
        req.query,
        canonical_headers,
        signed_headers,
        hex(&Sha256::digest(req.payload))
    );
    let scope = format!("{date}/{region}/{service}/aws4_request");
    let string_to_sign =
        format!("AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}", hex(&Sha256::digest(canonical_request.as_bytes())));
    let k_date = hmac(format!("AWS4{}", creds.secret_access_key).as_bytes(), date);
    let k_region = hmac(&k_date, region);
    let k_service = hmac(&k_region, service);
    let k_signing = hmac(&k_service, "aws4_request");
    let signature = hex(&hmac(&k_signing, &string_to_sign));

    let mut out = vec![
        ("x-amz-date", amz_date.to_string()),
        (
            "authorization",
            format!(
                "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed_headers}, Signature={signature}",
                creds.access_key_id
            ),
        ),
    ];
    if let Some(token) = &creds.session_token {
        out.push(("x-amz-security-token", token.clone()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // "get-vanilla" from the AWS SigV4 test suite.
    #[test]
    fn matches_the_aws_test_suite() {
        let creds = Credentials {
            access_key_id: "AKIDEXAMPLE".into(),
            secret_access_key: "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY".into(),
            session_token: None,
        };
        let req =
            Request { method: "GET", host: "example.amazonaws.com", path: "/", query: "", headers: &[], payload: b"" };
        let headers = sign(&creds, "us-east-1", "service", &req, "20150830T123600Z");
        let auth = &headers.iter().find(|(k, _)| *k == "authorization").unwrap().1;
        assert_eq!(
            auth,
            "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20150830/us-east-1/service/aws4_request, \
             SignedHeaders=host;x-amz-date, \
             Signature=5fa00fa31553b73ebf1942676e86291e8372ff2a2260956d9b8aae1d763fbf31"
        );
    }

    #[test]
    fn signs_the_session_token() {
        let creds =
            Credentials { access_key_id: "A".into(), secret_access_key: "S".into(), session_token: Some("T".into()) };
        let req = Request { method: "POST", host: "h", path: "/", query: "", headers: &[], payload: b"{}" };
        let headers = sign(&creds, "us-east-2", "ecr", &req, "20261009T000000Z");
        let auth = &headers.iter().find(|(k, _)| *k == "authorization").unwrap().1;
        assert!(auth.contains("SignedHeaders=host;x-amz-date;x-amz-security-token"), "{auth}");
        assert!(headers.iter().any(|(k, v)| *k == "x-amz-security-token" && v == "T"));
    }
}
