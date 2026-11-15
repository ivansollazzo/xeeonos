# Applicazioni XeeonOS (.xapp): specifiche

Una `.xapp` è **un'applicazione in un solo file**, come una `.app` del Mac:

- la metti in `~/Applications` e compare nel menu;
- la cancelli e sparisce dal menu (i suoi dati restano);
- la sostituisci con una versione nuova e si aggiorna;
- doppio clic per aprirla, anche da Scaricati.

Non ci sono store, repository o root. Ogni applicazione gira nella sua
**sandbox**, con i **permessi** che l'utente le dà, ed è **firmata** dal suo
autore. È del tutto separata da Flatpak: Flatpak e AppImage restano
supportati, ma per conto loro.

Il supporto sta nell'estensione inclusa e speciale `xapp`
(`extensions/xapp/`): il programma `xapp` (sorgente in `xapp/`, in Rust),
il monitor di `~/Applications`, il tipo di file e le miniature. Le ricette
delle applicazioni ufficiali stanno nel repository
[xeeonos-applications](https://github.com/ivansollazzo/xeeonos-applications).

## Indice

1. [Il modello: ogni app ha il suo mini-Arch](#1-il-modello-ogni-app-ha-il-suo-mini-arch)
2. [Il file .xapp](#2-il-file-xapp)
3. [Il manifest](#3-il-manifest)
4. [Firma e fiducia (il Gatekeeper di XeeonOS)](#4-firma-e-fiducia-il-gatekeeper-di-xeeonos)
5. [Dove stanno le cose](#5-dove-stanno-le-cose)
6. [Il monitor e il desktop](#6-il-monitor-e-il-desktop)
7. [L'avvio](#7-lavvio)
8. [La sandbox](#8-la-sandbox)
9. [Il contratto con il sistema](#9-il-contratto-con-il-sistema)
10. [I permessi](#10-i-permessi)
11. [La grafica (GPU)](#11-la-grafica-gpu)
12. [I dialoghi](#12-i-dialoghi)
13. [Aggiornamenti delle applicazioni](#13-aggiornamenti-delle-applicazioni)
14. [Costruire una .xapp (xapp build)](#14-costruire-una-xapp-xapp-build)
15. [Il comando xapp](#15-il-comando-xapp)
16. [Il codice](#16-il-codice)
17. [Test](#17-test)
18. [Limiti noti e lavori futuri](#18-limiti-noti-e-lavori-futuri)

---

## 1. Il modello: ogni app ha il suo mini-Arch

Sul Mac un'app usa i framework del sistema, che sono stabili. Su Linux il
sistema cambia a ogni snapshot di Arch, quindi una `.xapp` porta con sé il
proprio **sottosistema minimo**: un Arch ridotto all'osso, con solo i
pacchetti che servono a quell'applicazione, preso da uno snapshot
dell'[Arch Linux Archive](https://archive.archlinux.org) scelto da chi la
costruisce (anche vecchio).

- XeeonOS può aggiornarsi senza rompere le applicazioni: ognuna vede sempre
  il proprio mini-Arch.
- Non c'è un runtime condiviso da mantenere: chi pubblica l'app ne sceglie lo
  snapshot e ne è responsabile.
- Il sistema presta solo poche cose, elencate nel
  [contratto con il sistema](#9-il-contratto-con-il-sistema), più i driver
  grafici (sezione 11).

Lo stesso schema lo usa lo Steam Linux Runtime (pressure-vessel).

Prezzi accettati: un'app grande pesa 150-400 MB (le librerie sono ripetute
in ogni app), e un'app costruita su uno snapshot vecchio si tiene le
librerie di allora (`xapp info` avvisa oltre i 12 mesi).

## 2. Il file .xapp

```
  +-------------------------------+  offset 0
  | immagine erofs (sola lettura) |  il mini-Arch: /usr, /etc, /xapp/...
  | con il "magic" xapp nei primi |  (i primi 1024 byte, che erofs non usa)
  | byte                          |
  +-------------------------------+  dimensione dell'immagine
  | manifest.json                 |  cos'è l'applicazione (sezione 3)
  | firma (64 byte, Ed25519)      |  dei byte grezzi del manifest
  | icona (PNG o SVG)             |
  | trailer (32 byte)             |  dove stanno i pezzi
  +-------------------------------+  fine del file
```

- **Immagine erofs**: la stessa tecnologia di `/usr` e delle estensioni di
  XeeonOS. Si monta e si usa subito, senza estrazione né spazio doppio (come
  un `.dmg` aperto al volo), ed è in sola lettura per costruzione:
  l'applicazione non può modificarsi. Compressione lz4hc.
- **Magic**: `\x7fXAPP1\n\0` all'offset 0. Grazie a questo, il desktop
  riconosce una `.xapp` dal contenuto (`application/x-xapp`) anche se il
  file viene rinominato.
- **Trailer** (ultimi 32 byte): `XAPPEND1`, dimensione dell'immagine (u64),
  lunghezza del manifest (u32), lunghezza dell'icona (u32), 8 byte a zero.
  Tutti i numeri sono little-endian. Manifest, firma e icona si leggono
  dalla fine del file senza montare nulla.
- Limiti: manifest al massimo 1 MiB, icona al massimo 4 MiB.

Dentro l'immagine, `/xapp/packages.txt` elenca i pacchetti Arch installati.

## 3. Il manifest

JSON, scritto da `xapp build` e firmato dall'autore. Il formato è il
**formato 1**: tutti i campi previsti esistono già, così le versioni
successive di xapp aggiungono funzioni senza cambiare il formato.

| Campo | Significato |
|---|---|
| `format` | `1` |
| `app_id` | id in stile dominio inverso (`org.mozilla.firefox`): lettere, cifre, `.`, `_`, `-`, almeno due parti |
| `name`, `names` | nome, e nomi tradotti (`{"it": "..."}`) |
| `description` | descrizione breve |
| `version` | versione confrontabile (`146.0.1`) |
| `exec` | comando (percorso assoluto nell'immagine) e argomenti fissi |
| `arch` | `x86_64` |
| `snapshot` | snapshot di Arch dell'immagine (`2026-11-13`) |
| `host_contract` | versione del [contratto con il sistema](#9-il-contratto-con-il-sistema) che serve (`1`) |
| `gpu` | `host` (driver del sistema, predefinito) o `bundled` (driver propri) |
| `glibc`, `libstdcxx` | versioni di glibc e gcc-libs dell'immagine |
| `permissions` | i permessi che l'app **può** chiedere, ognuno con il motivo: `[{"id": "camera", "reason": "Per le videochiamate"}]` |
| `mime_types` | tipi di file che apre |
| `categories` | categorie del menu |
| `wm_class` | nome delle sue finestre (Wayland app id), per la dock |
| `icon_type`, `icon` | `png` o `svg`, e `{size, sha256}` dell'icona |
| `image` | `{size, sha256}` dell'immagine |
| `signer` | chiave pubblica Ed25519 dell'autore (64 cifre esadecimali) |

Un manifest di formato diverso, con un contratto più nuovo di quello del
sistema o con campi obbligatori sbagliati viene rifiutato.

## 4. Firma e fiducia (il Gatekeeper di XeeonOS)

- L'autore firma con Ed25519 i **byte grezzi** del manifest (niente JSON
  "normalizzato", quindi nessuna ambiguità).
- Il manifest contiene lo SHA-256 dell'immagine e dell'icona: la firma di
  pochi byte copre tutto il file. Un solo byte cambiato e l'app viene
  **rifiutata**, senza modo di forzarla.
- **Pinning**: la prima chiave vista per un `app_id` viene ricordata
  (`~/.local/state/xapps/keys/<id>`). Un file con lo stesso id e un'altra
  chiave viene rifiutato: potrebbe essere un falso aggiornamento.
- **Revoca**: `/usr/lib/xapp/revoked` (arriva con gli aggiornamenti di
  XeeonOS, quindi è firmata come loro) e `/etc/xapp/revoked`
  (dell'amministratore). Righe `key <chiave>`, `app <id>`,
  `image <sha256>`. Un'app revocata non si apre.
- **Autori fidati**: chiavi in `/usr/lib/xapp/trusted.d/*.pub` (con il
  sistema) e `/etc/xapp/trusted.d/*.pub` (amministratore). Le loro app si
  aprono subito.
- **Autore sconosciuto**: file firmato e integro, ma da una chiave che
  nessuno garantisce. Alla prima apertura c'è la domanda "Apri comunque",
  e la risposta viene ricordata per quella chiave
  (`~/.local/state/xapps/approved/<id>`).

Il controllo completo dell'immagine (circa un secondo per un'app grande)
viene ricordato per file, dimensione, data, inode e digest
(`~/.local/state/xapps/verified`), così l'avvio successivo non rilegge
tutto. `xapp verify` lo rifà sempre per intero.

## 5. Dove stanno le cose

Tutto è dell'utente, senza root:

| Percorso | Cosa |
|---|---|
| `~/Applications/` | i file `.xapp` (cartella vera, una per utente) |
| `~/.var/xapps/<id>/` | i dati dell'app, cioè la sua home nella sandbox. **Mai cancellati da soli**: rimettere il file riporta l'app com'era |
| `~/.local/state/xapps/` | registro (`apps/`), chiavi (`keys/`, `approved/`), permessi (`permissions/`), icone, versioni usate, cache dei controlli |
| `~/.local/share/applications/<id>.desktop` | voci di menu |
| `~/.config/autostart/<id>.desktop` | avvio al login (permesso `autostart`) |
| `$XDG_RUNTIME_DIR/xapp/` | immagini montate (`mnt/`), istanze in esecuzione (`run/`), dialoghi |

Del sistema (dentro l'estensione): `/usr/bin/xapp`,
`/usr/lib/systemd/user/xapp-sync.*`, `/usr/lib/xapp/` (chiavi fidate,
revoche), `/usr/share/xapp/xeeonos-xapp.xml` (tipo di file),
`/usr/share/applications/xapp-run.desktop` (doppio clic),
`/usr/share/thumbnailers/xapp.thumbnailer` (miniature in File).

## 6. Il monitor e il desktop

Il monitor è un servizio **utente** (systemd `--user`, nessun root):

- `xapp-sync.path` sorveglia `~/Applications` (e la crea se manca);
- `xapp-sync.service` esegue `xapp sync`: all'avvio della sessione e a ogni
  cambiamento della cartella;
- `xapp-sync.timer` ripete il controllo completo ogni 15 minuti, per quello
  che la sorveglianza può perdere.

`xapp sync`:

1. legge ogni `.xapp` della cartella. Un file ancora in copia è incompleto:
   viene riprovato dopo poco, fino a circa un minuto;
2. controlla firma, revoca, pinning e immagine (sezione 4);
3. se più file hanno lo stesso id, vince la versione più alta;
4. per ogni app scrive la voce di menu `~/.local/share/applications/<id>.desktop`
   (con `Exec=xapp run <id> %U`), l'icona e i tipi di file, e ricorda la
   chiave (pinning);
5. toglie le voci delle app il cui file non c'è più. I dati restano.

Il tipo di file `application/x-xapp` (per nome `*.xapp` e per magic) viene
copiato nel database dell'utente (`~/.local/share/mime`), perché quello del
sistema dentro `/usr` sigillato non si può ricostruire. Il gestore
predefinito è `xapp-run.desktop` (`xapp run %f`), aggiunto a
`~/.config/mimeapps.list` solo se l'utente non ha già scelto altro. Così il
**doppio clic** su una `.xapp` la apre. File mostra come miniatura l'icona
dell'app (`xapp thumbnail`).

Ogni file scritto da xapp porta `X-Xapp-Id=`: xapp cancella solo i propri.

## 7. L'avvio

`xapp run <file|id> [argomenti]` (anche dal menu o con un doppio clic):

1. **Controllo del file**: firma, revoca, pinning, immagine. Se fallisce,
   l'app viene rifiutata con un messaggio (in una finestra di dialogo se
   lanciata dal menu).
2. **Domande prima di aprire**, come sul Mac:
   - file fuori da `~/Applications`: "Apri solo questa volta" o "Sposta in
     Applicazioni e apri" (preselezionata), oppure "Annulla";
   - autore sconosciuto: "Apri comunque" o "Annulla" (preselezionata);
   - versione più vecchia di quella usata prima: conferma.
3. **Permessi** dichiarati e non ancora decisi, uno per uno, con il motivo
   scritto dall'app. Per ora le domande arrivano **all'avvio**.
4. **Sandbox**: monta l'immagine, prepara l'ambiente, avvia l'app in uno
   scope systemd tutto suo (`app-xapp-<id>-<pid>.scope`). Lo scope dà i
   limiti di risorse (TasksMax), il nome con cui il portal del desktop
   riconosce l'app, e i log.

## 8. La sandbox

È fatta con **bubblewrap** (la stessa base di Flatpak), senza podman,
distrobox o container. L'app parte in nuovi namespace (utente, mount, PID,
IPC, UTS, cgroup, e rete se non è permessa), su una radice vuota riempita
pezzo per pezzo:

| Nella sandbox | Da dove |
|---|---|
| `/usr`, `/opt`, ... e i link `/bin`, `/lib` | l'immagine dell'app, in sola lettura (montata con erofsfuse in `$XDG_RUNTIME_DIR/xapp/mnt/<digest>`) |
| `/etc` | quello dell'immagine, più quello che presta il sistema (sezione 9) e un `passwd`/`group` con il solo utente |
| `/proc`, `/dev`, `/sys` (solo dispositivi, sola lettura) | nuovi |
| `/tmp`, `/var`, `/run` | vuoti, privati |
| `$HOME` | `~/.var/xapps/<id>` (i dati dell'app), lettura e scrittura |
| `$HOME/Documenti`, ... | solo le cartelle permesse, allo stesso percorso |
| `/run/user/<uid>` | Wayland, PipeWire (se permesso), il bus di sessione filtrato |
| `/run/host` | font, icone, temi, driver grafici del sistema |
| `/run/xapp/docs/<n>/` | i file che l'app è stata chiamata ad aprire |

Altre protezioni:

- **seccomp**: un filtro BPF scritto a mano (`xapp-sandbox/src/seccomp.rs`)
  rifiuta ptrace, mount, moduli del kernel, kexec, keyring, BPF, perf,
  unshare, setns, clone con `CLONE_NEWUSER`, cambio dell'ora, swap, reboot,
  `TIOCSTI` e altro. clone3 risponde "non supportato", così glibc ripiega su
  clone, di cui il filtro vede i flag. I programmi i386/x32 vengono fermati.
- `--cap-drop ALL`, `--die-with-parent`.
- **Bus di sessione filtrato** con `xdg-dbus-proxy`, politica "nega tutto".
  Passano solo il proprio nome, il portal del desktop per le parti che
  chiedono all'utente o aprono soltanto (link e file, impostazioni come il
  tema scuro, inibizione della sospensione, condivisione dello schermo,
  stampa, e-mail), le notifiche e il portal Camera (con i rispettivi
  permessi). Altre app, GNOME Shell, i segreti e il file manager non
  esistono per l'app.
- Ambiente ripulito (`--clearenv`): passano solo lingua, terminale, desktop
  e fuso orario.
- **Niente X11** di default (permesso `x11`).

**File aperti**: un file passato all'app (doppio clic su un documento, o
argomento) viene prestato da solo in `/run/xapp/docs/<n>/<nome>`, anche se
la sua cartella non è permessa. Aprirlo è il consenso dell'utente, come il
"powerbox" del Mac. I file dentro una cartella permessa mantengono il loro
percorso.

Cosa la sandbox **non** protegge (detto chiaramente): la GPU e la tastiera
sono condivise con il resto della sessione. Su GNOME il socket Wayland è
quello normale (Mutter non dà comunque protocolli privilegiati ai client
normali). Un'app con permesso `x11` può vedere le altre finestre X11.

## 9. Il contratto con il sistema

È l'elenco esatto di quello che XeeonOS mette dentro ogni sandbox. Tutto il
resto viene dall'immagine dell'app. **Versione 1**:

- sempre: socket Wayland, bus di sessione filtrato, `/etc/hosts`,
  `/etc/hostname`, `/etc/machine-id`, fuso orario (`/etc/localtime`),
  certificati CA (`/etc/ssl`, `/etc/ca-certificates`), font del sistema e
  dell'utente (`/run/host/fonts`, via `FONTCONFIG_FILE`), icone e temi
  (`/run/host/share`, in `XDG_DATA_DIRS`), le lingue del sistema
  (`/usr/lib/locale`, se l'immagine ne ha la cartella), lingua e terminale;
- con il permesso `network`: la rete e il DNS (`/etc/resolv.conf`);
- con `gpu`: `/dev/dri` e, se l'app vuole `gpu: host`, lo stack grafico del
  sistema (sezione 11);
- con `audio`: PipeWire e PulseAudio; con `camera`: `/dev/video*`,
  `/dev/media*` e PipeWire;
- con le cartelle: le cartelle permesse.

Se il contratto cambia, cambia il numero (`host_contract` nel manifest):
un'app che chiede un contratto più nuovo di quello del sistema non si
apre.

## 10. I permessi

Sono come il TCC del Mac. L'app dichiara nel manifest i permessi che **può**
chiedere, ognuno con il motivo. Un permesso non dichiarato non si può mai
ottenere.

| Permesso | Cosa dà | Di default |
|---|---|---|
| `network` | rete e internet | sì |
| `gpu` | accelerazione grafica | sì |
| `notifications` | notifiche | sì |
| `documents`, `downloads`, `pictures`, `music`, `videos`, `desktop` | la cartella utente corrispondente (nomi tradotti da `~/.config/user-dirs.dirs`) | chiesto |
| `audio` | suono e microfono (in v1 insieme: il socket PipeWire non li separa bene) | chiesto |
| `camera` | fotocamera | chiesto |
| `x11` | finestre X11 (programmi vecchi, isolamento più debole) | chiesto |
| `autostart` | avvio al login | chiesto |

"Di default sì" vale per le app di un autore fidato o approvato, se l'utente
non ha detto di no. Le scelte stanno in
`~/.local/state/xapps/permissions/<id>` (righe `<permesso>=yes|no`) e si
cambiano con `xapp permissions <app> <permesso> yes|no|ask`. Quello che non
è permesso **non esiste** nella sandbox: una cartella negata non c'è
proprio. Se all'avvio non c'è nessuno a cui chiedere, i permessi in sospeso
non vengono dati per quella volta (e vengono richiesti la volta dopo).

## 11. La grafica (GPU)

Un mini-Arch vecchio ha una Mesa vecchia, che non conosce le schede video
nuove. Con `gpu: host` (il default) il sistema presta la sua Mesa, sempre la
più recente: OpenGL, EGL, GBM e i driver Vulkan, con tutte le librerie che
caricano (trovate con `ldd`), **glibc e libstdc++ comprese**. Lo fa anche
Steam.

- Funziona perché una glibc nuova fa girare programmi compilati per una
  vecchia: vince sempre la versione più nuova. Prima si controlla che glibc
  e libstdc++ del sistema siano almeno nuove quanto quelle dell'app;
  altrimenti l'app tiene i suoi driver (o disegna senza accelerazione) e
  xapp lo dice.
- Ogni libreria prestata sostituisce il file con lo stesso nome dell'app
  (montata sopra) oppure, se l'app non ce l'ha, va in `/run/host/gpu/lib`
  (in `LD_LIBRARY_PATH`). Il set di glibc arriva sempre tutto dal sistema.
- I driver vanno in `/run/host/gpu` (`LIBGL_DRIVERS_PATH`,
  `GBM_BACKENDS_PATH`, `__EGL_VENDOR_LIBRARY_DIRS`). Le descrizioni dei
  driver Vulkan vengono riscritte con il solo nome della libreria
  (`VK_DRIVER_FILES`).

Con `gpu: bundled` l'app usa i driver della propria immagine.

## 12. I dialoghi

Per ora sono **dialoghi da terminale**, disegnati con `whiptail` in bianco e
nero come quelli di xctl:

1. se `xapp` gira in un terminale, le domande compaiono lì;
2. se l'app parte dal menu o con un doppio clic, si apre una finestra di
   terminale (GNOME Console) con le domande. xapp scrive le domande in
   `$XDG_RUNTIME_DIR/xapp/dialogs/<id>.questions`, la finestra esegue
   `xapp dialog <file>` e scrive le risposte accanto. Se la finestra viene
   chiusa senza rispondere, vale come "no";
3. senza terminale né finestra non c'è risposta: vale come "no".

Variabili: `XAPP_DIALOG=text` (righe di testo semplici invece di whiptail),
`XAPP_TERMINAL=<programma>` (il terminale da aprire, `none` per nessuno),
`--yes` o `XAPP_ASSUME_YES=1` (risponde a tutto con la prima scelta, per
script e CI). Più avanti i dialoghi potranno diventare finestre grafiche
senza cambiare il resto.

## 13. Aggiornamenti delle applicazioni

Si aggiorna **sostituendo il file** in `~/Applications`:

- stesso `app_id` e stessa chiave (pinning) = aggiornamento;
- se ci sono più file, vince la versione più alta;
- una versione più vecchia di quella già usata chiede conferma all'avvio;
- la nuova immagine ha un altro digest e quindi un'altra cartella di
  montaggio: le istanze già aperte continuano con la vecchia finché non si
  chiudono.

L'app non può aggiornarsi da sola (la sua immagine è in sola lettura): gli
aggiornamenti interni, come quello di Firefox, vanno spenti nella ricetta.
Gli aggiornamenti dalla rete non sono previsti per ora.

## 14. Costruire una .xapp (xapp build)

Una **ricetta** è una cartella con `recipe.json`:

```json
{
  "app_id": "org.mozilla.firefox",
  "name": "Firefox",
  "names": {"it": "Firefox"},
  "description": "Browser web",
  "snapshot": "2026/11/13",
  "packages": ["firefox"],
  "exec": ["/usr/lib/firefox/firefox"],
  "gpu": "host",
  "permissions": [{"id": "network", "reason": "Per navigare"}],
  "mime_types": ["text/html"],
  "categories": ["Network", "WebBrowser"],
  "wm_class": "firefox",
  "icon": "/usr/share/icons/hicolor/128x128/apps/firefox.png",
  "remove": ["usr/lib/firefox/updater"],
  "script": "build.sh"
}
```

- `version` si può omettere: è la versione del primo pacchetto.
- `icon`: un percorso dentro l'immagine (inizia con `/`) o un file della
  ricetta.
- `script`: un passo della ricetta, eseguito con `ROOT=<radice>` dopo
  l'installazione dei pacchetti (file aggiuntivi, impostazioni).
- `remove`: percorsi da togliere dalla radice.

`xapp build <ricetta>`:

1. in un container Arch Linux (podman o docker; `XAPP_CONTAINER` per
   sceglierlo) installa con `pacstrap` i pacchetti **dallo snapshot della
   ricetta**, con i loro hook (cache delle librerie, icone, font, schemi
   GSettings), senza documentazione, header e librerie statiche;
2. toglie quello che serve solo a un sistema intero (`/boot`, `/home`,
   `/var`, ...);
3. crea l'immagine erofs, scrive il magic, calcola gli hash, scrive il
   manifest, lo firma e chiude il file;
4. lo installa in `~/Applications` (`--no-install` per non farlo).

La chiave dell'autore: `--key <file>` (creata se manca), oppure il testo
della chiave in `XAPP_SIGNING_KEY`, oppure `~/.config/xapp/signing.key`
(creata la prima volta). **Va conservata**: gli aggiornamenti di un'app
devono avere la stessa chiave. `--root <cartella>` impacchetta una radice
già pronta, senza container.

## 15. Il comando xapp

```
xapp run [--yes] <file|app> [argomenti]   avvia (anche quello che fa il doppio clic)
xapp install [--yes] <file>               copia in ~/Applications
xapp build <ricetta> [--key <file>] [--output <file>] [--no-install] [--root <dir>]
xapp list                                 le applicazioni di ~/Applications
xapp info <app|file>                      cos'è, autore, snapshot, permessi
xapp verify <file>                        firma e integrità, senza aprire
xapp remove <app> [--purge]               toglie il file (i dati restano; --purge: anche quelli)
xapp permissions <app> [<permesso> yes|no|ask]
xapp logs <app> [-f]                      i messaggi dell'app (journal)
```

Interni, usati dal sistema: `xapp sync` (monitor), `xapp dialog <file>`
(finestra dei dialoghi), `xapp thumbnail <file> <png> [dim]` (miniature).

Non c'è nessun repository o store: l'applicazione si scarica da internet
come su macOS, poi si apre o si installa. `xapp` è un comando a sé, non è
dentro xctl, e non richiede sudo.

## 16. Il codice

`xapp/` è un workspace Cargo con crate separate, come xctl:

| Crate | Compito |
|---|---|
| `xapp-core` | errori, programmi esterni, percorsi, dialoghi (`ui.rs`), versioni, date |
| `xapp-format` | il file: manifest e contenitore (immagine + trailer) |
| `xapp-trust` | chiavi Ed25519, firma, autori fidati, pinning, revoca |
| `xapp-permissions` | catalogo dei permessi e scelte dell'utente |
| `xapp-sandbox` | montaggio, seccomp, filtro D-Bus, GPU, riga di comando di bubblewrap, avvio nello scope |
| `xapp-desktop` | registro, voci di menu, tipo di file, `xapp sync` |
| `xapp-build` | `xapp build` (con `build-root.sh` per il container) |
| `xapp` | il programma: legge la riga di comando |

Dipendenze esterne, poche e note: `serde_json` (JSON), `sha2` (SHA-256),
`ed25519-dalek` (firme). Cargo.lock le fissa e la build usa `--locked`.
Il binario viene compilato dentro l'estensione (`extensions/xapp/build.sh`,
con il Rust dello stesso snapshot). Programmi del sistema usati:
`bwrap`, `erofsfuse`/`fusermount3`, `xdg-dbus-proxy`, `systemd-run`,
`whiptail`, `update-mime-database`, per le miniature SVG `rsvg-convert`,
e per `xapp build` `podman` o `docker`.

## 17. Test

- Unitari (`cargo test` nel job "xapp (format, lint, unit tests)"): file
  `.xapp` scritto e riletto, un byte cambiato rifiutato, manifest,
  firma, filtro seccomp eseguito da un piccolo interprete BPF, regole del
  bus, prestito dei file, mimeapps.list, versioni e date.
- In VM (`tools/ci/xapp.sh`, job "Test xapp"): l'app di prova
  (`tools/ci/xapp-hello/`) costruita con `xapp build`, messa in
  `~/Applications` durante una sessione GNOME. Si verificano: il menu, il
  tipo di file anche senza estensione, il doppio clic; nella sandbox:
  immagine propria, sistema e home invisibili, cartella permessa sì e negata
  no, niente rete, niente namespace utente, `/usr` in sola lettura,
  Wayland, driver grafici del sistema (llvmpipe nella VM), bus filtrato
  (portal sì, GNOME Shell no), file aperto prestato. Vengono rifiutati un
  file cambiato, la copia di un altro autore e un'app revocata. Per un
  autore sconosciuto si vede la domanda, e senza nessuno a cui chiedere
  l'app non si apre. Infine miniatura, lista, info, e la rimozione che
  toglie la voce dal menu e lascia i dati.

## 18. Limiti noti e lavori futuri

- **Accesso a un singolo file** scelto nel selettore di sistema (document
  portal): serve che xdg-desktop-portal riconosca le app xapp (patch o
  supporto upstream). Oggi i permessi sono per cartella, più i file
  prestati all'apertura.
- **Microfono separato dalla riproduzione** (regole WirePlumber o portal).
- **Domande al momento dell'uso** invece che all'avvio.
- **Dialoghi grafici** invece che da terminale.
- **security-context-v1** di Wayland, se Mutter lo supporterà.
- **Chiave XeeonOS** per le app ufficiali di xeeonos-applications (la
  pubblica in `/usr/lib/xapp/trusted.d/`, la privata come segreto della
  CI), e rotazione delle chiavi.
- **Deduplicazione** tra immagini, aggiornamenti dalla rete, app condivise
  tra utenti, app in GNOME Software: possibili senza cambiare il formato.
