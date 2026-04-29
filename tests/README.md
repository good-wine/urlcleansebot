# Test Suite - Work in Progress

## Status

I test sono stati creati ma necessitano di adattamento alle API esistenti del progetto.

## Azioni Necessarie

### 1. Verificare API Database

I test usano metodi che potrebbero non esistere:

- `increment_link_count()` → Verificare metodo corretto nel database
- `get_user_stats()` → Potrebbe essere `get_user_config()`
- `add_to_history()` → Verificare con `log_cleaned_link()`
- `set_user_lang()` → Implementare o usare `set_user_language()`

### 2 Aggiornare Test Fixtures

Il file `tests_disabled_temporarily/common/mod.rs` usa campi Config che potrebbero essere cambiati:

```rust
// Da verificare:
webhook_url: None,      // Potrebbe essere server_addr
webhook_port: 8443,     // Potrebbe non esistere
cookie_key: "...",      // Verificare se esiste
```

### 3. Aggiornare Sanitizer Tests

Il `RuleEngine` potrebbe non avere il metodo `clean_url()` direttamente:

```rust
// Da verificare l'API corretta:
rules.clean_url(url).await  // Potrebbe essere diverso
```

## Come Abilitare i Test

1. Rinomina la directory:

   ```bash
   cd /workspaces/clearurlsbot
   mv tests_disabled_temporarily tests
   ```

2. Esamina le API esistenti:

   ```bash
   # Cerca metodi disponibili
   grep -r "pub async fn" src/db/implementation.rs
   ```

3. Aggiorna i test per usare le API corrette

4. Esegui i test:

   ```bash
   cargo test
   ```

## Struttura Test Creata

```
tests_disabled_temporarily/
├── common/
│   └── mod.rs              # Test utilities
├── sanitizer_tests.rs      # URL sanitization tests (da fixare)
├── database_tests.rs       # Database operations tests (da fixare)
└── bot_commands_tests.rs   # Bot command tests (da fixare)
```

## Esempio Test Corretto

```rust
#[tokio::test]
async fn test_user_config() {
    let db = Db::new("sqlite::memory:").await.unwrap();
    let user_id = 12345;
    
    // Usa API esistente
    let config = db.get_user_config(user_id).await.unwrap();
    assert_eq!(config.user_id, user_id);
}
```

## Prossimi Passi

1. Documentare tutte le API pubbliche in `src/db/implementation.rs`
2. Creare fixtures di test basate sulle API reali
3. Implementare test incrementalmente
4. Aggiungere test integration per comandi bot

---

**Nota**: I test sono una base di partenza. Devono essere adattati al codebase esistente.

