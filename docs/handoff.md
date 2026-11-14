# XeeonOS: come andare avanti da solo

Guida per Ivan (13 novembre 2026): lo stato del progetto, il lavoro di ogni
giorno, versioni e canali, cosa resta da fare.

## Dove siamo

- **Sistema base** (fase 1): immagine sigillata (`/usr` erofs + dm-verity),
  aggiornamenti A/B firmati da ghcr (`xctl update`), ritorno automatico alla
  versione precedente, canali (`xctl channel`), installazione dalla ISO,
  Recovery, cifratura LUKS2, snapshot dei dati, firewall. Manca solo la tua
  prova dal vivo di `sudo xctl update` su un sistema installato.
- **Secure Boot e TPM** (fase 2): le build pubblicate sono firmate con la tua
  chiave. Mancano i passi sul PC vero (registrare la chiave nel firmware,
  `sudo xctl encryption tpm on`) e il backup della chiave di firma degli
  aggiornamenti.
- **Desktop** (fase 3): GNOME su Wayland con GDM, audio PipeWire,
  Bluetooth; modulo `tools` (SSH, server spento, `xctl ssh-server on|off`; FUSE 2 per le AppImage); estensioni di
  sistema (`xctl extensions`: flatpak e printing incluse; quelle opzionali, podman,
  distrobox, docker, rocm, rocm-sdk e touchpad T14, nel repository
  `xeeonos-extensions`).
  Le app `.xapp` sono da ripensare da zero.

Ogni cosa ha la sua pagina in `docs/` (in inglese): `modules.md` è l'indice
dei moduli, `extensions.md` delle estensioni.

## Il lavoro di ogni giorno

1. Un ramo per ogni modifica, partendo da `testing`.
2. Una pull request verso `testing` (per una correzione già uscita su stable,
   poi un backport verso `stable`). La CI (`.github/workflows/build.yml`)
   costruisce tutto e lancia un test per funzione (`tools/ci/<nome>.sh`), in
   parallelo: circa un'ora e mezza.
3. Se un test fallisce: apri il job, leggi la fine del log (la console della
   macchina virtuale è copiata lì; i log completi sono negli artifact
   `serial-logs-<test>`). Correggi e fai push: la CI riparte.
   - Se hai cambiato solo test o documentazione dopo un run **fallito o
     annullato**, la CI riusa le immagini e rifà solo i test (molto più
     veloce). Un push annulla il run in corso: correggi appena capisci il
     problema, senza aspettare gli altri test.
   - Nelle pull request girano davvero solo i test toccati dalla modifica
     (`tools/ci/affects.txt`); gli altri diventano verdi subito. Dopo il
     merge, sul ramo, girano sempre tutti.
   - Un test che fallisce per caso: "Re-run failed jobs", una volta sola.
4. Tutto verde: "Ready for review" → "Merge pull request" → "Confirm merge" →
   "Delete branch".
5. Dopo il merge, la CI di `testing` pubblica la nuova versione su
   `ghcr.io/ivansollazzo/xeeonos:testing`, firmata.

Un test nuovo: uno script in `tools/ci/`, una riga nella matrice di
`build.yml`, una riga in `tools/ci/README.md`, e dopo il merge il suo nome
("Test <nome>") tra i controlli richiesti della ruleset "Experimental branch
protection" (Settings → Rules → Rulesets).

Per provare a mano un'immagine: scarica l'artifact `disk-image` o
`xeeonos-iso` del run e avviala in una macchina virtuale UEFI.

## Snapshot di Arch

Lo snapshot lo scegli tu, niente lo sposta da solo: Actions → **Nuovo
snapshot** → "Run workflow". Scegli il ramo (`testing`, dove vanno i nuovi
snapshot) e la data (`AAAA/MM/GG`, oppure vuota per
il più recente). Si apre (o si aggiorna) la pull request "Arch snapshot
<data>", con le notizie di Arch e i pacchetti che cambiano. Se è in bozza,
leggi le notizie prima. Se la CI è verde, uniscila; se lo snapshot non ti
convince, non unirla e rilancia con un'altra data.

Serve una volta sola il secret `XEEONOS_BOT_TOKEN`: GitHub → il tuo avatar →
Settings → Developer settings → Fine-grained tokens → Generate new token;
i repository xeeonos e xeeonos-extensions; permessi "Contents" e "Pull
requests" in lettura e scrittura. Poi nel repository: Settings → Secrets and
variables → Actions → New repository secret, nome `XEEONOS_BOT_TOKEN`.
Senza, il workflow dice solo cosa farebbe.

## Versioni e canali

Le versioni seguono il mese (`AA.MM`: 26.11, a gennaio 27.1). Ramo = canale
(dettagli in `docs/updates.md`):

| Ramo | Cosa | Versioni |
|---|---|---|
| `testing` | quello che verrà: novità, nuovi snapshot; ogni merge pubblica | `26.12-testing.7` |
| `stable` | versioni fisse; solo backport da testing | `26.12.1`, `26.12.2` |

`testing` è il ramo principale. Quando vuoi aprire `stable`:

1. Su GitHub, nel menu dei rami, scrivi `stable` e scegli "Create branch
   stable from testing".
2. Controlla che `stable` sia tra i rami della ruleset (Settings → Rules →
   Rulesets → Target branches).
3. Da lì in poi: le novità vanno in `testing` con una pull request; le
   correzioni anche, e se servono su stable una seconda pull request verso
   `stable` (backport).

**Pubblicare una versione stable**: Actions → **Pubblica versione** → "Run
workflow" scegliendo il ramo `stable`. Il workflow calcola il numero
(`AA.MM.S`: S cresce a ogni pubblicazione del mese, poi riparte: 26.12.1,
26.12.2, 27.1.1), costruisce, fa tutti i test e, solo se è tutto verde,
pubblica la versione firmata sul canale `stable` e crea la release GitHub
`v26.12.2` con la ISO (segnata "Latest"; crea anche il tag `v26.12.2`).
Ogni merge su `testing` mette la sua ISO in una pre-release
(`v26.12-testing.7`): resta solo l'ultima.
Se qualcosa è rosso non esce niente: correggi su `stable` e rilancia. I
merge su `stable` da soli non pubblicano niente.

Un sistema cambia canale con `sudo xctl channel set testing` (o `stable`), poi `sudo xctl update`.

Le estensioni sono compatibili esattamente con una versione: ogni versione
pubblicata ha le sue (a breve dal repository `xeeonos-extensions`, che a
ogni build apre da solo la sua pull request e la unisce se è verde).

## Le chiavi

- Firma degli aggiornamenti: secret `XEEONOS_SIGNING_KEY` + la tua copia.
  Fingerprint `58AE3043…A7ACBBA98DC0A7D8`. Se la perdi, i sistemi installati
  non accettano più aggiornamenti: tienine una copia fuori dal PC.
- Secure Boot: secret `XEEONOS_SECUREBOOT_KEY` + la tua copia (certificato
  fino al 2046).
- Mai metterle nel repository o in un messaggio.

## Cosa resta da fare

In ordine di importanza:

1. Prova su hardware vero: installazione, aggiornamento dal vivo, Secure
   Boot, TPM, sospensione, Wi-Fi, il touchpad del T14 con l'estensione
   (da `xeeonos-extensions`).
2. Backup della chiave di firma fuori dal PC.
3. `.xapp`: da ripensare da zero (repository `xeeonos-applications`).
4. Flatpak preinstallati (al primo avvio o nella ISO): da decidere.
5. Installatore grafico in Qt (oggi `xctl install-system` testuale).
6. Logo e grafica (schermata di avvio, sfondo, schermata di login).
7. Estensioni nuove: nel repository `xeeonos-extensions` (le incluse,
   flatpak e printing, restano in `extensions/` qui).

## Idee e decisioni prese (da non rimettere in discussione senza motivo)

- Base Arch con snapshot datato, mkosi, A/B `/usr`, UKI, systemd-boot con
  tre tentativi; niente pacman nell'immagine.
- Solo GPU Intel, AMD e Nouveau.
- NetworkManager; Btrfs su LUKS2 sempre (password spenta = chiave vuota).
- App: native minime, Flatpak per il resto; `.xapp` da ripensare.
- Estensioni: incluse (nell'immagine, Special) o opzionali (scaricate,
  firmate) o da file (non verificate, con avviso).
- Nessuna compatibilità all'indietro necessaria finché si installa da zero.
