# Semantica e decisioni

## Modello logico

Pianificazione classica deterministica, osservabile, sequenziale, a costo unitario.
Dominio finito di oggetti nominati; tipi piatti con supertipo implicito `object`.
Nessuna funzione sui termini: termini = oggetti oppure variabili. Nomi unici
(UNA), chiusura del dominio e mondo chiuso (CWA). Uno stato contiene esattamente
gli atomi ground veri; gli altri sono falsi.

Formule: atomi, uguaglianza, negazione, congiunzione, disgiunzione, implicazione,
quantificatori esistenziali e universali tipati. Variabili libere ammesse solo
come parametri delle azioni. Gli obiettivi devono essere chiusi: le variabili
esistenziali delle slide sono rese esplicite. Quantificazione su tipo vuoto:
`forall` vero, `exists` falso. Scope lessicale, incluso shadowing.

## Transizioni e FOL

`Poss(a,s) ↔ pre(a)` valutata nell'interpretazione finita di `s`.
`Result(s,a) = (s \\ Del(a)) ∪ Add(a)`; in caso di sovrapposizione prevale Add.
Effetti simultanei, solo atomi positivi in Add/Del, variabili legate dai parametri.
Persistenza degli altri fluenti risolve il frame problem. Per ogni atomo ground F:

`F(do(a,s)) ↔ F ∈ Add(a) ∨ (F(s) ∧ F ∉ Del(a))`.

Il piano produce un testimone costruttivo `do(a_n, ... do(a_1, S0))` per
`∃s Goal(s)`, verificato riproducendo le transizioni. **Non** è un dimostratore
generale per FOL né un motore di risoluzione sul situation calculus: la FOL
serve per specificare e valutare precondizioni e obiettivi su modelli finiti.

## Ricerca

BFS in avanti, azioni ground ottenute dal prodotto cartesiano dei domini tipati,
insieme visitati per stati canonici, predecessori per ricostruire il piano.
Piani minimi nel numero di azioni; completezza se i limiti non intervengono.
Esiti distinti: piano (anche vuoto), irraggiungibile dopo esaurimento del grafo,
limite di stati raggiunto, errore di modello o grounding eccessivo.

Limiti configurabili di stati memorizzati e azioni ground prevengono crescita
illimitata delle strutture principali. Non sono limiti temporali: valutare
quantificatori annidati può comunque costare molto. BFS è didattica, non adatta
a istanze industriali. Nessuna euristica o variante semantica nascosta.

## Esclusioni

Niente fluenti numerici, costi/durate, funzioni, effetti condizionali o quantificati,
predicati derivati, gerarchie di tipi, incertezza, concorrenza, SAT/SMT esterni.
Costrutti non supportati devono generare errori, mai essere ignorati.
