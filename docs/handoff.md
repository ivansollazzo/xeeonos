# XeeonOS: come andare avanti da solo

Guida per Ivan (13 novembre 2026): lo stato del progetto, il lavoro di ogni
giorno, la prima alpha, cosa resta da fare.

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
- **Desktop** (fase 3): Plasma su Wayland con SDDM, audio PipeWire,
  Bluetooth; modulo `tools` (podman, distrobox, SSH spento); estensioni di
  sistema (`xctl ext`: flatpak e printing incluse, docker, rocm, rocm-sdk,
  touchpad T14 opzionali); `.xapp` sperimentale con Firefox.

Ogni cosa ha la sua pagina in `docs/` (in inglese): `modules.md` è l'indice
dei moduli, `extensions.md` delle estensioni.

## Il lavoro di ogni giorno

1. Un ramo per ogni modifica, partendo da `experimental`.
2. Una pull request verso `experimental`. La CI (`.github/workflows/build.yml`)
   costruisce tutto e lancia un test per funzione (`tools/ci/<nome>.sh`), in
   parallelo: circa un'ora e mezza.
3. Se un test fallisce: apri il job, leggi la fine del log (la console della
   macchina virtuale è copiata lì; i log completi sono negli artifact
   `serial-logs-<test>`). Correggi e fai push: la CI riparte.
   - Se hai cambiato solo test o documentazione dopo un run **fallito**, la CI
     riusa le immagini e rifà solo i test (molto più veloce).
   - Un test che fallisce per caso: "Re-run failed jobs", una volta sola.
4. Tutto verde: "Ready for review" → "Merge pull request" → "Confirm merge" →
   "Delete branch".
5. Dopo il merge, la CI di `experimental` pubblica la nuova versione su
   `ghcr.io/ivansollazzo/xeeonos:experimental`, firmata.

Un test nuovo: uno script in `tools/ci/`, una riga nella matrice di
`build.yml`, una riga in `tools/ci/README.md`, e dopo il merge il suo nome
("Test <nome>") tra i controlli richiesti della ruleset "Experimental branch
protection" (Settings → Rules → Rulesets).

Per provare a mano un'immagine: scarica l'artifact `disk-image` o
`xeeonos-iso` del run e avviala in una macchina virtuale UEFI.

## Aggiornamento di Arch ogni settimana

Ogni lunedì `.github/workflows/snapshot.yml` apre (o aggiorna) la pull
request "Arch snapshot <data>", con le notizie di Arch e i pacchetti che
cambiano. Se è in bozza, leggi le notizie prima. Se la CI è verde, uniscila;
se la settimana sembra rischiosa, lasciala aperta: il lunedì dopo passa a una
data più nuova.

Serve una volta sola il secret `XEEONOS_BOT_TOKEN`: GitHub → il tuo avatar →
Settings → Developer settings → Fine-grained tokens → Generate new token;
solo il repository xeeonos; permessi "Contents" e "Pull requests" in
lettura e scrittura. Poi nel repository: Settings → Secrets and variables →
Actions → New repository secret, nome `XEEONOS_BOT_TOKEN`. Senza, il
workflow dice solo cosa farebbe.

## La prima alpha (0.1-alpha)

Quando `experimental` è come la vuoi:

1. Su GitHub, nel menu dei rami, scrivi `0.1-alpha` e scegli "Create branch
   0.1-alpha from experimental".
2. La CI parte da sola e, se è verde, pubblica il canale `0.1-alpha`
   (versioni `0.1.0-alpha.<n>`). La ruleset protegge già i rami `*-alpha`.
3. Un sistema passa all'alpha con `sudo xctl channel set 0.1-alpha` (o
   `alpha`, che segue sempre l'alpha più nuova), poi `sudo xctl update`.
4. Correzioni: pull request verso `0.1-alpha` (e la stessa correzione verso
   `experimental`, se serve anche lì).
5. Più avanti: `0.1-beta`, poi `0.1-stable` (in `mkosi.version` resta
   `0.1.0`; la stable usa il numero esatto, quindi le correzioni dopo
   diventano `0.1.1`, `0.1.2`...). Per la 0.2: cambia `mkosi.version` in
   `0.2.0` su `experimental`.

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
   Boot, TPM, sospensione, Wi-Fi, il touchpad del T14 con l'estensione.
2. Backup della chiave di firma fuori dal PC.
3. `.xapp`: firma dei pacchetti, isolamento (sandbox), un repository di app,
   il formato definitivo; oggi è sperimentale.
4. Flatpak preinstallati (al primo avvio o nella ISO): da decidere.
5. Installatore grafico in Qt (oggi `xctl install-system` testuale).
6. Logo e grafica (schermata di avvio, sfondo, SDDM).
7. Estensioni nuove: una cartella in `extensions/` (vedi
   `docs/extensions.md`, "Making one").

## Idee e decisioni prese (da non rimettere in discussione senza motivo)

- Base Arch con snapshot datato, mkosi, A/B `/usr`, UKI, systemd-boot con
  tre tentativi; niente pacman nell'immagine.
- Solo GPU Intel, AMD e Nouveau.
- NetworkManager; Btrfs su LUKS2 sempre (password spenta = chiave vuota).
- App: native minime, Flatpak per il resto, `.xapp` in `/var/lib/xapps`.
- Estensioni: incluse (nell'immagine, Special) o opzionali (scaricate,
  firmate) o da file (non verificate, con avviso).
- Nessuna compatibilità all'indietro necessaria finché si installa da zero.
