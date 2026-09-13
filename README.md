# ics-next

Answers one question: what's the next event in this calendar file?

I got tired of opening a whole calendar app just to check "is that meeting
tomorrow at 9 or 10". This is a command-line tool that reads a `.ics` file
and prints the next upcoming event, or lists everything upcoming with
`--all`. Nothing else.

It has its own iCalendar parser, written by hand, with no dependencies. The
one thing I actually care about here is that when a `.ics` file is broken,
the error tells you exactly where: line, column, the offending line printed
back at you, and a caret under the problem.

## Usage

```
$ ics-next calendar.ics
2025-01-15 09:00:00 UTC  Team standup

$ ics-next calendar.ics --all
2025-01-15 09:00:00 UTC  Team standup
2025-01-17 14:00:00 UTC  Dentist
2025-02-01 00:00:00 UTC  Conference (all day)

$ ics-next calendar.ics --at 20250116T000000Z
2025-01-17 14:00:00 UTC  Dentist
```

Given a file like:

```
BEGIN:VCALENDAR
VERSION:2.0
BEGIN:VEVENT
UID:standup-1@example.com
SUMMARY:Team standup
DTSTART:20250115T090000Z
END:VEVENT
END:VCALENDAR
```

### When something is wrong with the file

```
$ ics-next broken.ics
broken.ics: line 6, column 9: invalid DTSTART: expected an 8-digit date (YYYYMMDD), found "2025-01-15"
  DTSTART:2025-01-15
        ^
```

Same treatment for structural problems — a `BEGIN:VEVENT` that never gets
closed, an `END:VALARM` that shows up where an `END:VEVENT` was expected, a
content line missing its `:` separator. The column always points at the
actual problem, not just the start of the line.

## What it does not do (yet)

- No `RRULE` support — recurring events are read as their single `DTSTART`
  occurrence and nothing else.
- No timezone resolution. `TZID` parameters are parsed and then ignored;
  every date-time is compared as if it were UTC. Fine if your calendar is
  all UTC or all one timezone; wrong otherwise. This is the main thing I'd
  fix next.
- Line folding (long values wrapped across multiple physical lines) is
  unfolded correctly for parsing, but error columns past the first physical
  line of a folded value are clamped to the end of that first line rather
  than tracked precisely.

## Building

Standard library only, no external crates:

```
cargo build --release
```

## License

MIT, see LICENSE.
