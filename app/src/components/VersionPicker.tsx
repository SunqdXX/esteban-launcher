import { useEffect, useMemo, useRef, useState } from "react";
import type { PinnedVersion, Release } from "../api";
import { cx } from "../cx";
import PixelIcon, { CHEVRON_DOWN, CHEVRON_UP } from "./PixelIcon";
import styles from "./VersionPicker.module.css";

interface VersionPickerProps {
  releases: Release[];
  pinned: PinnedVersion[];
  value: string;
  disabled?: boolean;
  compact?: boolean;
  opensUp?: boolean;
  onChange: (id: string) => void;
}

interface Row {
  id: string;
  note: string;
}

export default function VersionPicker({ releases, pinned, value, disabled, compact, opensUp, onChange }: VersionPickerProps) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const root = useRef<HTMLDivElement>(null);
  const tag = pinned.find((p) => p.id === value)?.tag ?? "";

  const groups = useMemo(() => {
    const q = query.trim().toLowerCase();
    const match = (id: string) => q === "" || id.toLowerCase().includes(q);
    const ours: Row[] = pinned.filter((p) => match(p.id)).map((p) => ({ id: p.id, note: p.tag }));
    const all: Row[] = releases
      .filter((r) => !r.pinned && match(r.id))
      .map((r) => ({ id: r.id, note: r.date.slice(0, 4) }))
      .sort((a, b) => Number(!a.id.startsWith(q)) - Number(!b.id.startsWith(q)));
    return { ours, all };
  }, [pinned, releases, query]);

  useEffect(() => {
    if (!open) return;
    const close = (event: MouseEvent) => {
      if (root.current && !root.current.contains(event.target as Node)) setOpen(false);
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    window.addEventListener("mousedown", close);
    window.addEventListener("keydown", escape);
    return () => {
      window.removeEventListener("mousedown", close);
      window.removeEventListener("keydown", escape);
    };
  }, [open]);

  const pick = (id: string) => {
    setOpen(false);
    setQuery("");
    if (id !== value) onChange(id);
  };

  const option = (row: Row) => (
    <li key={row.id}>
      <button
        type="button"
        role="option"
        aria-selected={row.id === value}
        className={cx(styles.option, row.id === value && styles.on)}
        onClick={() => {
          pick(row.id);
        }}
      >
        <span className={styles.v}>{row.id}</span>
        {row.note && <span className={styles.tag}>{row.note}</span>}
      </button>
    </li>
  );

  return (
    <div ref={root} className={cx(styles.picker, compact && styles.compact)}>
      {open && (
        <div className={cx(styles.list, opensUp ? styles.up : styles.down)}>
          <input
            className={styles.search}
            autoFocus
            value={query}
            placeholder="Search versions"
            aria-label="Search versions"
            onChange={(e) => {
              setQuery(e.target.value);
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                const first = groups.ours[0] ?? groups.all[0];
                if (first) pick(first.id);
              }
            }}
          />
          <div className={styles.scroll}>
            <ul className={styles.group} role="listbox" aria-label="Minecraft version">
              {groups.ours.length > 0 && <li className={styles.heading}>Esteban versions</li>}
              {groups.ours.map(option)}
              {groups.all.length > 0 && <li className={styles.heading}>All releases</li>}
              {groups.all.map(option)}
              {groups.ours.length + groups.all.length === 0 && <li className={styles.none}>No release matches {query}</li>}
            </ul>
          </div>
        </div>
      )}
      <button
        type="button"
        className={cx(styles.select, open && styles.open)}
        disabled={disabled}
        aria-haspopup="listbox"
        aria-expanded={open}
        onClick={() => {
          setOpen(!open);
        }}
      >
        <span className={styles.v}>{value}</span>
        {tag && <span className={styles.selectTag}>{tag}</span>}
        <span className={styles.chev}>
          <PixelIcon cells={open ? CHEVRON_UP : CHEVRON_DOWN} />
        </span>
      </button>
    </div>
  );
}
