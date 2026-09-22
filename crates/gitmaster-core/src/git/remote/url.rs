//! 远端地址解析与固定脱敏展示。
use url::Url;

/// 对允许的 HTTPS、SSH 和 scp 简写地址展示去除用户信息后的地址。
pub(super) fn display(raw: &str) -> String {
    match parse(raw) {
        Ok(mut url) => {
            let _ = url.set_username("");
            url.to_string()
        }
        Err(()) => "远端地址不可安全显示".to_owned(),
    }
}

/// 保守接受明确的网络地址；不把 helper、选项或本地路径转换为 SSH。
fn parse(raw: &str) -> Result<Url, ()> {
    if raw.is_empty()
        || raw.trim() != raw
        || raw.chars().any(char::is_control)
        || raw.contains('\\')
    {
        return Err(());
    }
    if !raw.contains("://") {
        let (authority, path) = raw.split_once(':').ok_or(())?;
        if path.is_empty() || path.starts_with(':') || path.contains(['?', '#']) {
            return Err(());
        }
        let (user, host) = authority
            .rsplit_once('@')
            .map_or((None, authority), |(u, h)| (Some(u), h));
        if host.is_empty()
            || host.starts_with('-')
            || host.contains(['/', '@', ' ', '?', '#'])
            || (host.len() == 1 && host.as_bytes()[0].is_ascii_alphabetic())
            || user.is_some_and(|u| {
                u.is_empty() || u.starts_with('-') || u.contains(['/', '@', ' ', ':'])
            })
        {
            return Err(());
        }
        let mut url = Url::parse(&format!("ssh://{host}/{}", path.trim_start_matches('/')))
            .map_err(|_| ())?;
        if let Some(user) = user {
            url.set_username(user).map_err(|_| ())?;
        }
        return Ok(url);
    }
    let url = Url::parse(raw).map_err(|_| ())?;
    let authority = raw
        .split_once("://")
        .ok_or(())?
        .1
        .split(['/', '?', '#'])
        .next()
        .ok_or(())?;
    if !matches!(url.scheme(), "https" | "ssh")
        || url.host_str().is_none()
        || (url.scheme() == "https" && authority.contains('@'))
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.host_str().is_some_and(|host| host.starts_with('-'))
    {
        return Err(());
    }
    Ok(url)
}

/// 网络动作再次验证原始地址；保留 scp 的相对路径语义，不用展示 URL 执行。
pub(super) fn validate(raw: &str) -> Result<(), super::OperationError> {
    parse(raw)
        .map(|_| ())
        .map_err(|_| super::OperationError::new("UNSUPPORTED_TRANSPORT"))
}
