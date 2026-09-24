# Piano di verifica

## Accettazione

- Parsing DSL e PDDL in uno stesso Task, errori localizzati dove possibile.
- FOL: CWA, UNA, not/and/or/imply, quantificatori, tipi vuoti, scope lessicale.
- Validazione: dichiarazioni duplicate, tipi/predicati/oggetti sconosciuti,
  arità, variabili libere, effetti e inizializzazione mal tipati.
- BFS: piano minimo, replay valido, frame, add prevale su delete, piano vuoto,
  ciclo senza soluzione, limiti distinti dall'impossibilità.
- Esempi eseguiti davvero: viaggio, blocchi, logistica, quantificatori;
  modelli PDDL completi, almeno una istanza impossibile e una già soddisfatta.
- Rustdoc con esempi compilabili e guida DSL incorporata.
- `cargo test`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo doc --no-deps`; CLI eseguita su tutti gli esempi.

## Consegne incrementali

1. KB Markdown/Mermaid prima dell'implementazione.
2. Modello condiviso e motore FOL/BFS.
3. Parser DSL/PDDL, CLI, esempi e documentazione.
4. Test integrati, correzioni emerse e verbale delle esecuzioni.

Deleghe richieste dall'utente: sottoagenti `gpt-6-luna`, reasoning `high`.
Responsabilità separate per motore, parser, CLI/documentazione; integrazione e
verifica finale a cura dell'agente principale. I sottoagenti non fanno commit.
