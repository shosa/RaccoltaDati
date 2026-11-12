# Changelog

Rebuild of Esprit "Raccolta dati" 10.11.1.0. Dates are when the change reached the work-PC
package (`pacchetto/`).

## 2026-11-12

- **VERSAMENTI** tab in place of ARTICOLI: what was recorded in a day on the workstation's
  machines, with totals, pieces per machine, "solo i miei" and previous/next day. Same filters as
  the original's "Pezzi registrati" (SPC excluded, PD_PROGR order).
- **ARTICOLI** moved to the third sub-tab of COMMESSE.
- Documentation: README (maintainers), GUIDA-OPERATORE (operators), this changelog.

## 2026-11-10

- **Launcher**: the clicked program starts the app as a child, waits for its window and
  restarts it in another graphics mode if it dies or hangs (dx12 → opengl → vulkan → software,
  up to 8 attempts); logs exit codes, .NET 4.6.2 and the installed original's version.
- Graphics fallback modes, including a software mode needing no video driver.
- Smaller program (no debug info), no console, one copy at a time.
- **Full screen** option (settings, F11).
- **INFO COMMESSA**: which machine of this workstation an order is on (reads only).
- **PIÙ COMMESSE**: collect many scanned orders, one run per R order, one save per group.
- After recording quantities only the recorded list reloads (as the original).
- Connect timeout 15 s (the original's default).

## 2026-11-06 / 2026-11-09

- Login gate before any data; operator code and password with letters; closing the app ends
  the shifts started on this PC; PRENDI MACCHINA moved to MACCHINE; no machine pre-selected.
- Esprit.ini editor on the login screen (backup, yes/no switches, hidden passwords).
- Touch scrolling of every list; flat look.
- BARCODE button and listening window; DATI tab removed.

## 2026-11-05

- First build: start-up splash with log, operators, machines, orders and phases, quantity
  screen, saves through the original's libraries (RdPonte), automatic barcode flow.
