# Supported Languages

URLCleanseBot supports 15 languages out of the box. The language is **auto-detected** from your Telegram client settings — no manual configuration needed.

## Language List

| Code | Language | Native Name | Flag |
|------|----------|-------------|------|
| `it` | Italian | Italiano | 🇹 |
| `en` | English | English | 🇬🇧 |
| `es` | Spanish | Español | 🇪🇸 |
| `fr` | French | Français | 🇫🇷 |
| `de` | German | Deutsch | 🇩🇪 |
| `pt` | Portuguese | Português | 🇧🇷 |
| `ru` | Russian | Русский | 🇷🇺 |
| `ar` | Arabic | العربية | 🇸🇦 |
| `hi` | Hindi | हिन्दी | 🇮🇳 |
| `zh` | Chinese | 中文 | 🇨🇳 |
| `ja` | Japanese | 日本語 | 🇯🇵 |
| `ko` | Korean | 한국어 | 🇰🇷 |
| `tr` | Turkish | Türkçe | 🇹🇷 |
| `nl` | Dutch | Nederlands | 🇳🇱 |
| `pl` | Polish | Polski | 🇵🇱 |

## How Language Detection Works

The bot resolves the language in this order:

1. **Telegram client language** — uses the language code from your Telegram app (highest priority)
2. **Default** — English if nothing else matches

Language is detected fresh on every interaction. There is no saved preference — to change the bot's language, simply change your Telegram app's language setting.

## Translation Guide

### Adding a New Language

1. Open `src/i18n.rs`
2. Add the new field to the `Translations` struct (if needed)
3. Add a new match arm with all translation fields:

```rust
"xx" => Translations {
    cleaning_feedback: "...",
    error_feedback: "...",
    // ... all fields
},
```

4. Add the language code to `SUPPORTED_LANGUAGES` in `helpers.rs`:
```rust
pub const SUPPORTED_LANGUAGES: &[&str] = &["it", "en", ..., "xx"];
```

5. Run `cargo fmt`, `cargo clippy`, and `cargo test`.

### Field Categories

| Prefix | Category | Count |
|--------|----------|-------|
| (none) | Core messages (welcome, help, stats) | 17 |
| `d_` | Dashboard strings | 13 |
| `s_` | Settings and UI labels | 40 |
| `rk_` | Reply keyboard buttons | 4 |
| `sec_` | Security alert messages | 10 |
| `err_` | Error messages | 5 |
| `info_` | Info/status messages | 4 |
| (misc) | Other (group_activated, truncated, etc.) | 16 |

**Total: ~109 fields per language**
