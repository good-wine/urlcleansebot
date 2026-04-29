//! Security scanning module for checking URLs against VirusTotal and URLScan.io
//!
//! This module provides functions to scan URLs for malicious content using
//! external security services. It consolidates results from multiple sources
//! into unified security reports.
//!
//! # Environment Variables
//!
//! - `VIRUSTOTAL_API_KEY` - API key for VirusTotal service
//! - `URLSCAN_API_KEY` - API key for URLScan.io service
//! - `VIRUSTOTAL_ALERT_ONLY` - If set, only return alerts for threats (default: true)
//! - `URLSCAN_ALERT_ONLY` - If set, only return alerts for threats (default: true)
use base64::prelude::*;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use regex::Regex;
use reqwest;
use serde_json;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio;
use tracing;

use crate::http_utils::retry_http_request;

/// Check URL with both VirusTotal and URLScan services and consolidate results
///
/// This function calls both security scanning services and combines their results
/// into a single consolidated alert message instead of sending separate messages.
/// Returns Option<String> with the combined alert if either service detects a threat.
pub async fn check_url_combined(url: &str) -> Option<String> {
    // Call both services concurrently for efficiency
    let vt_result = check_url_virustotal(url);
    let urlscan_result = check_url_urlscan(url);

    let (vt_msg, urlscan_msg) = tokio::join!(vt_result, urlscan_result);

    // Only send a message if at least one service detected a threat
    if vt_msg.is_none() && urlscan_msg.is_none() {
        return None;
    }

    // Build the consolidated message
    let mut consolidated = String::from(
        "🚨 <b>ALLERTA SICUREZZA</b> 🚨\n\
        ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n\
        🔴 <b>MINACCIA RILEVATA - REPORT CONSOLIDATO</b>\n\n",
    );

    // Extract key information from VirusTotal alert if present
    if let Some(vt_alert) = vt_msg {
        consolidated.push_str("🛡️ <b>VirusTotal Security Scan:</b>\n");
        // Extract the relevant part after the header
        if let Some(content_start) = vt_alert.find("🔴 <b>LINK PERICOLOSO RILEVATO</b>") {
            let content = &vt_alert[content_start..];
            // Get lines up to the report link
            if let Some(report_idx) = content.find("📋 <a href=") {
                let summary = &content[..report_idx];
                consolidated.push_str(summary);
                // Extract and append the report link
                if let Some(link_end) = content[report_idx..].find("</a>") {
                    consolidated.push_str(&content[report_idx..report_idx + link_end + 4]);
                }
            } else {
                consolidated.push_str(content);
            }
        }
        consolidated.push_str("\n\n");
    }

    // Extract key information from URLScan alert if present
    if let Some(urlscan_alert) = urlscan_msg {
        consolidated.push_str("🌐 <b>URLScan.io Web Reputation:</b>\n");
        // Extract the relevant part after the header
        if let Some(content_start) = urlscan_alert.find("🔴 <b>LINK PERICOLOSO RILEVATO</b>") {
            let content = &urlscan_alert[content_start..];
            // Get lines up to the report link
            if let Some(report_idx) = content.find("📋 <a href=") {
                let summary = &content[..report_idx];
                consolidated.push_str(summary);
                // Extract and append the report link
                if let Some(link_end) = content[report_idx..].find("</a>") {
                    consolidated.push_str(&content[report_idx..report_idx + link_end + 4]);
                }
            } else {
                consolidated.push_str(content);
            }
        }
        consolidated.push_str("\n\n");
    }

    // Add final warning
    consolidated.push_str(
        "⚠️ <b>ATTENZIONE:</b> Questo link è stato segnalato come pericoloso.\n\
        Si consiglia di NON visitare la pagina.",
    );

    Some(consolidated)
}

/// Check URL with VirusTotal API v3
///
/// Returns a user-facing VirusTotal message with scan outcome.
/// Requires VIRUSTOTAL_API_KEY environment variable.
pub async fn check_url_virustotal(url: &str) -> Option<String> {
    let alert_only = std::env::var("VIRUSTOTAL_ALERT_ONLY")
        .ok()
        .map(|value| {
            let normalized = value.trim().to_ascii_lowercase();
            !matches!(normalized.as_str(), "0" | "false" | "no" | "off")
        })
        .unwrap_or(true);

    let api_key = match std::env::var("VIRUSTOTAL_API_KEY") {
        Ok(key) if !key.is_empty() && key != "your_virustotal_api_key_here" => key,
        _ => {
            tracing::debug!("VirusTotal: API key non configurata, scansione disabilitata");
            return None;
        }
    };

    tracing::info!(url = %url, "VirusTotal: Scansione in corso...");

    let encoded_url = BASE64_URL_SAFE_NO_PAD.encode(url);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .ok()?;

    let endpoint = format!("https://www.virustotal.com/api/v3/urls/{encoded_url}");

    let mut lookup_id = encoded_url.clone();

    let mut resp = match retry_http_request(
        || client.get(&endpoint).header("x-apikey", &api_key),
        "VirusTotal lookup",
    )
    .await
    {
        Ok(response) => response,
        Err(e) => {
            tracing::warn!(error = %e, url = %url, "VirusTotal: richiesta fallita");
            if alert_only {
                return None;
            }
            return Some(
                "⚠️ <b>VirusTotal</b>\nImpossibile raggiungere il servizio. Riprova tra qualche minuto.".to_string(),
            );
        }
    };

    // Check if URL already exists in VirusTotal database
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        tracing::info!(url = %url, "VirusTotal: URL non presente, invio per analisi");

        let submit_resp = match retry_http_request(
            || {
                client
                    .post("https://www.virustotal.com/api/v3/urls")
                    .header("x-apikey", &api_key)
                    .form(&[("url", url)])
            },
            "VirusTotal submit",
        )
        .await
        {
            Ok(response) => response,
            Err(e) => {
                tracing::warn!(error = %e, url = %url, "VirusTotal: submit fallito");
                if alert_only {
                    return None;
                }
                return Some(
                    "⚠️ <b>VirusTotal: invio analisi fallito</b>\nRiprova tra qualche minuto."
                        .to_string(),
                );
            }
        };

        if submit_resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            tracing::warn!(url = %url, "VirusTotal: rate limit raggiunto");
            if alert_only {
                return None;
            }
            return Some("⏱️ <b>VirusTotal: limite richieste raggiunto</b>\nAttendi circa 1 minuto e riprova.".to_string());
        }

        if !submit_resp.status().is_success() {
            tracing::warn!(status = %submit_resp.status(), url = %url, "VirusTotal: submit API error");
            if alert_only {
                return None;
            }
            return Some(format!(
                "⚠️ <b>VirusTotal: errore API</b>\nCodice: {}",
                submit_resp.status()
            ));
        }

        if let Ok(submit_json) = submit_resp.json::<serde_json::Value>().await {
            if let Some(id) = submit_json["data"]["id"].as_str() {
                lookup_id = id.to_string();
            }
        }

        tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
        let submitted_endpoint = format!("https://www.virustotal.com/api/v3/urls/{lookup_id}");
        resp = match client
            .get(&submitted_endpoint)
            .header("x-apikey", &api_key)
            .send()
            .await
        {
            Ok(response) => response,
            Err(e) => {
                tracing::warn!(error = %e, url = %url, "VirusTotal: recupero report fallito dopo submit");
                if alert_only {
                    return None;
                }
                return Some("ℹ️ <b>VirusTotal</b>\nURL inviato per analisi. Report non ancora disponibile, riprova tra poco.".to_string());
            }
        };
    }

    if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        tracing::warn!(url = %url, "VirusTotal: rate limit raggiunto");
        if alert_only {
            return None;
        }
        return Some(
            "⏱️ <b>VirusTotal: limite richieste raggiunto</b>\nAttendi circa 1 minuto e riprova."
                .to_string(),
        );
    }

    if !resp.status().is_success() {
        tracing::warn!(status = %resp.status(), url = %url, "VirusTotal API error");
        if alert_only {
            return None;
        }
        return Some(format!(
            "⚠️ <b>VirusTotal: errore API</b>\nCodice: {}",
            resp.status()
        ));
    }

    // URL already exists in VirusTotal, use existing scan results
    if resp.status() == reqwest::StatusCode::OK && resp.status() != reqwest::StatusCode::NOT_FOUND {
        tracing::info!(url = %url, "VirusTotal: Scansione precedente trovata, utilizzo risultati");
    }

    let json: serde_json::Value = match resp.json().await {
        Ok(value) => value,
        Err(e) => {
            tracing::warn!(error = %e, url = %url, "VirusTotal: risposta JSON non valida");
            if alert_only {
                return None;
            }
            return Some(
                "⚠️ <b>VirusTotal</b>\nImpossibile leggere la risposta dell'analisi.".to_string(),
            );
        }
    };

    // Parse detection stats
    let stats = &json["data"]["attributes"]["last_analysis_stats"];
    let malicious = stats["malicious"].as_i64().unwrap_or(0);
    let suspicious = stats["suspicious"].as_i64().unwrap_or(0);
    let harmless = stats["harmless"].as_i64().unwrap_or(0);
    let undetected = stats["undetected"].as_i64().unwrap_or(0);
    let total = harmless + malicious + suspicious + undetected;

    // Get last analysis date if available
    let last_analysis_date = json["data"]["attributes"]["last_analysis_date"]
        .as_i64()
        .and_then(|ts| {
            let analysis_time = UNIX_EPOCH + Duration::from_secs(ts as u64);
            SystemTime::now()
                .duration_since(analysis_time)
                .ok()
                .map(|elapsed| {
                    let hours = elapsed.as_secs() / 3600;
                    if hours < 1 {
                        "meno di 1 ora fa".to_string()
                    } else if hours < 24 {
                        format!("{} ore fa", hours)
                    } else {
                        format!("{} giorni fa", hours / 24)
                    }
                })
        });

    if malicious > 0 || suspicious > 2 {
        tracing::warn!(
            malicious = malicious,
            suspicious = suspicious,
            harmless = harmless,
            total = total,
            url = %url,
            "VirusTotal: Minaccia rilevata!"
        );

        let report_link = format!("https://www.virustotal.com/gui/url/{}", encoded_url);

        let msg = if malicious > 0 {
            let mut alert = format!(
                "🚨 <b>ALLERTA SICUREZZA</b> 🚨\n\
                ━━━━━━━━━━━━━━━━\n\
                🛡️ <b>VirusTotal Security Scan</b>\n\n\
                🔴 <b>LINK PERICOLOSO RILEVATO</b>\n\n\
                📊 <b>Risultati Scansione:</b>\n\
                🔴 Dannoso: <b>{}</b> motori\n",
                malicious
            );
            if suspicious > 0 {
                alert.push_str(&format!("🟡 Sospetto: <b>{}</b> motori\n", suspicious));
            }
            alert.push_str(&format!(
                "✅ Sicuro: <b>{}</b> motori\n\
                ⚪️ Non rilevato: {} motori\n\
                📈 Rilevazioni: <b>{}/{}</b> motori\n",
                harmless,
                undetected,
                malicious + suspicious,
                total
            ));
            if let Some(date) = last_analysis_date {
                alert.push_str(&format!("\n🕐 Ultima analisi: <i>{}</i>\n", date));
            }
            alert.push_str(&format!(
                "\n🔒 <b>ATTENZIONE: NON APRIRE QUESTO LINK!</b>\n\
                Contiene contenuti potenzialmente dannosi.\n\n\
                📋 <a href=\"{}\">Visualizza Report Dettagliato ›</a>",
                report_link
            ));
            alert
        } else {
            let mut warning = format!(
                "⚠️ <b>AVVISO SICUREZZA</b>\n\
                ━━━━━━━━━━━━━━━━\n\
                🛡️ <b>VirusTotal Security Scan</b>\n\n\
                🟡 <b>Link classificato come SOSPETTO</b>\n\n\
                📊 <b>Risultati Scansione:</b>\n\
                🟡 Sospetto: <b>{}</b> motori\n\
                ✅ Sicuro: <b>{}</b> motori\n\
                ⚪️ Non rilevato: {} motori\n\
                📈 Rilevazioni sospette: <b>{}/{}</b> motori\n",
                suspicious, harmless, undetected, suspicious, total
            );
            if let Some(date) = last_analysis_date {
                warning.push_str(&format!("\n🕐 Ultima analisi: <i>{}</i>\n", date));
            }
            warning.push_str(&format!(
                "\n⚠️ <b>Procedere con CAUTELA</b>\n\
                Questo link potrebbe non essere sicuro.\n\n\
                📋 <a href=\"{}\">Visualizza Report Dettagliato ›</a>",
                report_link
            ));
            warning
        };
        Some(msg)
    } else {
        tracing::info!(
            total = total,
            harmless = harmless,
            url = %url,
            "VirusTotal: URL sicuro (nessuna minaccia rilevata)"
        );
        if alert_only {
            return None;
        }

        let mut msg = format!(
            "✅ <b>URL VERIFICATO SICURO</b>\n\
            ───────────────────\n\
            🛡️ <b>VirusTotal Security Scan</b>\n\n\
            📊 <b>Risultati Scansione:</b>\n\
            ✅ Sicuro: <b>{}</b> motori\n\
            ⚪️ Non rilevato: {} motori\n\
            📈 Totale verifiche: <b>{}</b> motori\n",
            harmless, undetected, total
        );

        if let Some(date) = last_analysis_date {
            msg.push_str(&format!("\n🕐 Ultima analisi: <i>{}</i>\n", date));
        }

        msg.push_str(&format!(
            "\n✨ Nessuna minaccia rilevata\n\
            📋 <a href=\"https://www.virustotal.com/gui/url/{}\">Visualizza Report ›</a>",
            encoded_url
        ));

        Some(msg)
    }
}

/// Search for existing URLScan.io scans of a URL.
/// Returns the UUID of an existing scan if found, None otherwise.
pub async fn search_existing_urlscan(url: &str, api_key: &str) -> Option<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .ok()?;

    // URLScan Search API: search for the exact URL using query parameter
    let search_query = format!("domain:{}", url.split('/').nth(2).unwrap_or(url));

    let search_resp = match retry_http_request(
        || {
            client
                .get("https://urlscan.io/api/v1/search/")
                .header("API-Key", api_key)
                .query(&[("q", search_query.as_str())])
        },
        "URLScan search",
    )
    .await
    {
        Ok(response) => response,
        Err(_) => return None,
    };

    if !search_resp.status().is_success() {
        return None;
    }

    let search_json: serde_json::Value = match search_resp.json().await {
        Ok(value) => value,
        Err(_) => return None,
    };

    // Get the first result (most recent) that matches the exact URL
    if let Some(results) = search_json["results"].as_array() {
        for result in results {
            if let Some(page_url) = result["page"]["url"].as_str() {
                if page_url == url {
                    if let Some(uuid) = result["_id"].as_str() {
                        tracing::info!(url = %url, uuid = %uuid, "URLScan.io: Scansione precedente trovata");
                        return Some(uuid.to_string());
                    }
                }
            }
        }
    }

    None
}

/// Check URL with URLScan.io API.
///
/// Returns a user-facing URLScan.io message with scan outcome.
/// Requires URLSCAN_API_KEY environment variable.
pub async fn check_url_urlscan(url: &str) -> Option<String> {
    let alert_only = std::env::var("URLSCAN_ALERT_ONLY")
        .ok()
        .map(|value| {
            let normalized = value.trim().to_ascii_lowercase();
            !matches!(normalized.as_str(), "0" | "false" | "no" | "off")
        })
        .unwrap_or(true);

    let api_key = match std::env::var("URLSCAN_API_KEY") {
        Ok(key) if !key.is_empty() && key != "your_urlscan_api_key_here" => key,
        _ => {
            tracing::debug!("URLScan.io: API key non configurata, scansione disabilitata");
            return None;
        }
    };

    tracing::info!(url = %url, "URLScan.io: Scansione in corso...");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .ok()?;

    // First, try to find an existing scan
    let mut uuid = search_existing_urlscan(url, &api_key).await;
    let mut result_link = "https://urlscan.io".to_string();

    // If not found, submit a new scan
    if uuid.is_none() {
        let submit_resp = match retry_http_request(
            || {
                client
                    .post("https://urlscan.io/api/v1/scan/")
                    .header("API-Key", &api_key)
                    .json(&serde_json::json!({ "url": url, "visibility": "private" }))
            },
            "URLScan submit",
        )
        .await
        {
            Ok(response) => response,
            Err(e) => {
                tracing::warn!(error = %e, url = %url, "URLScan.io: richiesta fallita");
                if alert_only {
                    return None;
                }
                return Some(
                    "⚠️ <b>URLScan.io non raggiungibile</b>\nRiprova tra qualche minuto."
                        .to_string(),
                );
            }
        };

        if submit_resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            tracing::warn!(url = %url, "URLScan.io: rate limit raggiunto");
            if alert_only {
                return None;
            }
            return Some(
                "⏱️ <b>URLScan.io: limite richieste raggiunto</b>\nAttendi e riprova.".to_string(),
            );
        }

        if !submit_resp.status().is_success() {
            let status_code = submit_resp.status();

            // Try to extract error details from response body
            let error_details = if let Ok(error_body) = submit_resp.text().await {
                // Check for specific error messages from URLScan.io

                // Technical errors that should respect alert_only mode
                if error_body.contains("URL is too long") || error_body.contains("length") {
                    tracing::warn!(url = %url, "URLScan.io: URL troppo lungo");
                    if alert_only {
                        return None;
                    }
                    return Some(
                        "⚠️ <b>ERRORE SCANSIONE</b>\n\
                        ━━━━━━━━━━━━━━━━\n\
                        📌 <b>URLScan.io</b>\n\n\
                        🔗 <b>URL troppo lungo</b>\n\n\
                        ℹ️ Questo link è troppo lungo per essere scansionato.\n\n\
                        💡 <b>Suggerimento:</b>\n\
                        Prova ad accorciare l'URL usando un servizio\n\
                        di URL shortener (es: bit.ly, tinyurl, ecc.)"
                            .to_string(),
                    );
                }

                // URLScan blocked the scan for technical reasons (not because URL is malicious)
                if error_body.contains("Scan prevented")
                    || error_body.contains("blocked from scanning")
                    || error_body.contains("URL was blocked")
                {
                    tracing::warn!(
                        url = %url,
                        error = %error_body,
                        "URLScan.io: Scansione bloccata per motivi tecnici (non sicurezza)"
                    );
                    // This is a technical limitation, not a security alert
                    // Always suppress this in alert_only mode
                    if alert_only {
                        return None;
                    }
                    // In full report mode, still don't show as security alert
                    // Just log it and skip
                    return None;
                }

                error_body
            } else {
                "Unknown error".to_string()
            };

            tracing::warn!(
                status = %status_code,
                error = %error_details,
                url = %url,
                "URLScan.io API error"
            );

            if alert_only {
                return None;
            }
            return Some(format!(
                "⚠️ <b>ERRORE SCANSIONE</b>\n\
                ━━━━━━━━━━━━━━━━\n\
                📌 <b>URLScan.io</b>\n\n\
                🔧 <b>Errore Tecnico</b>\n\n\
                <b>Codice errore:</b> {}\n\n\
                ℹ️ <i>Il servizio ha incontrato un errore durante la scansione.</i>\n\n\
                💡 <b>Prova:</b>\n\
                • Riprova tra qualche minuto\n\
                • Verifica che l'URL sia valido\n\
                • Contatta l'admin se il problema persiste",
                status_code
            ));
        }

        let submit_json: serde_json::Value = match submit_resp.json().await {
            Ok(value) => value,
            Err(e) => {
                tracing::warn!(error = %e, url = %url, "URLScan.io: risposta submit non valida");
                if alert_only {
                    return None;
                }
                return Some(
                    "⚠️ <b>ERRORE SCANSIONE</b>\n\
                ━━━━━━━━━━━━━━━━\n\
                📌 <b>URLScan.io</b>\n\n\
                🔧 <b>Risposta non valida</b>\n\n\
                ℹ️ <i>Il servizio ha dato una risposta non riconoscibile.</i>\n\n\
                💡 <b>Prova:</b>\n\
                • Riprova tra 1-2 minuti\n\
                • Assicurati che l'URL sia valido"
                        .to_string(),
                );
            }
        };

        uuid = submit_json["uuid"].as_str().map(ToString::to_string);
        result_link = submit_json["result"]
            .as_str()
            .map(ToString::to_string)
            .unwrap_or_else(|| "https://urlscan.io".to_string());

        if uuid.is_none() {
            if alert_only {
                return None;
            }
            return Some(format!(
                "🕐 <b>ANALISI IN CORSO</b>\n\
                ━━━━━━━━━━━━━━━━\n\
                📌 <b>URLScan.io</b>\n\n\
                ⏳ <b>URL inviato per analisi</b>\n\n\
                ℹ️ <i>La scansione è in corso sul servizio.</i>\n\n\
                📋 <a href=\"{}\">Apri il report completo ›</a>\n\n\
                💡 <b>Nota:</b> Il rapporto sarà disponibile tra pochi istanti.",
                result_link
            ));
        }

        // Wait a bit for the scan to start processing before polling
        tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
    }

    let uuid_ref = uuid.as_ref()?;

    let uuid_re = Regex::new(
        r"(?i)^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$",
    )
    .ok()?;
    if !uuid_re.is_match(uuid_ref) {
        return None;
    }
    let safe_uuid = utf8_percent_encode(uuid_ref, NON_ALPHANUMERIC).to_string();

    let mut malicious = false;
    let mut potentially_malicious = false;
    let mut score = 0.0_f64;

    for _ in 0..4 {
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        let mut result_endpoint = match reqwest::Url::parse("https://urlscan.io/") {
            Ok(url) => url,
            Err(_) => continue,
        };
        {
            let mut segments = match result_endpoint.path_segments_mut() {
                Ok(path) => path,
                Err(_) => continue,
            };
            segments.extend(["api", "v1", "result", &safe_uuid, ""]);
        }
        let result_resp = match client
            .get(result_endpoint)
            .header("API-Key", &api_key)
            .send()
            .await
        {
            Ok(response) => response,
            Err(_) => continue,
        };

        if !result_resp.status().is_success() {
            continue;
        }

        let result_json: serde_json::Value = match result_resp.json().await {
            Ok(value) => value,
            Err(_) => continue,
        };

        malicious = result_json["verdicts"]["overall"]["malicious"]
            .as_bool()
            .unwrap_or(false);
        let verdict_text = result_json["verdicts"]["overall"]["verdict"]
            .as_str()
            .or_else(|| result_json["verdicts"]["overall"]["classification"].as_str())
            .or_else(|| result_json["verdicts"]["overall"]["label"].as_str())
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();

        potentially_malicious =
            verdict_text.contains("potentially malicious") || verdict_text.contains("suspicious");

        score = result_json["verdicts"]["overall"]["score"]
            .as_f64()
            .unwrap_or(0.0);
        break;
    }

    if malicious || potentially_malicious {
        tracing::warn!(
            url = %url,
            score = score,
            malicious = malicious,
            potentially_malicious = potentially_malicious,
            "URLScan.io: minaccia rilevata"
        );

        let verdict_label = if malicious {
            "MALICIOUS"
        } else {
            "POTENTIALLY MALICIOUS"
        };

        let msg = format!(
            "🚨 <b>ALLERTA SICUREZZA</b> 🚨\n\
            ━━━━━━━━━━━━━━━━\n\
            🌐 <b>URLScan.io Web Reputation</b>\n\n\
            🔴 <b>LINK PERICOLOSO RILEVATO</b>\n\n\
            📊 <b>Analisi Comportamentale:</b>\n\
            📈 Risk Score: <b>{:.1}/100</b>\n\
            🔴 Classificato come: <b>{}</b>\n\
            \n🔒 <b>ATTENZIONE:</b> Pagina web sospetta\n\
            Potrebbe contenere phishing o malware.\n\n\
            📋 <a href=\"{}\">Visualizza Scansione Completa ›</a>",
            score, verdict_label, result_link
        );

        return Some(msg);
    }

    tracing::info!(url = %url, score = score, "URLScan.io: URL senza segnali critici");
    if alert_only {
        return None;
    }

    let safety_level = if score == 0.0 {
        "✅ <b>COMPLETAMENTE SICURO</b>"
    } else if score < 25.0 {
        "✅ <b>BASSO RISCHIO</b>"
    } else {
        "🟢 <b>ACCETTABILE</b>"
    };

    Some(format!(
        "✅ <b>URL VERIFICATO</b>\n\
        ━━━━━━━━━━━━━━━━\n\
        🌐 <b>URLScan.io Web Reputation</b>\n\n\
        {}\n\n\
        📊 <b>Analisi Comportamentale:</b>\n\
        📈 Risk Score: <b>{:.1}/100</b>\n\
        🔍 Status: Nessuna minaccia rilevata\n\n\
        ✨ Pagina web verificata sicura\n\
        📋 <a href=\"{}\">Visualizza Scansione ›</a>",
        safety_level, score, result_link
    ))
}
