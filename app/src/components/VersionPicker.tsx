import { useEffect, useRef, useState } from "react";
import type { GameVersion } from "../api";
import { cx } from "../cx";
import PixelIcon, { CHEVRON_DOWN, CHEVRON_UP } from "./PixelIcon";
import styles from "./VersionPicker.module.css";

interface VersionPickerProps {
  versions: GameVersion[];
  value: string;
  disabled?: boolean;
  compact?: boolean;
  opensUp?: boolean;
  onChange: (id: string) => void;
}

export default function VersionPicker({ versions, value, disabled, compact, opensUp, onChange }: VersionPickerProps) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const current = versions.find((v) => v.id === value);

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

  return (
    <div ref={root} className={cx(styles.picker, compact && styles.compact)}>
      {open && (
        <ul className={cx(styles.list, opensUp ? styles.up : styles.down)} role="listbox" aria-label="Minecraft version">
          {versions.map((v) => (
            <li key={v.id}>
              <button
                type="button"
                role="option"
                aria-selected={v.id === value}
                className={cx(styles.option, v.id === value && styles.on)}
                onClick={() => {
                  setOpen(false);
                  if (v.id !== value) onChange(v.id);
                }}
              >
                <span className={styles.v}>{v.id}</span>
                {v.tag && <span className={styles.tag}>{v.tag}</span>}
              </button>
            </li>
          ))}
        </ul>
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
        <span className={styles.v}>{current?.id ?? value}</span>
        {current?.tag && <span className={styles.selectTag}>{current.tag}</span>}
        <span className={styles.chev}>
          <PixelIcon cells={open ? CHEVRON_UP : CHEVRON_DOWN} />
        </span>
      </button>
    </div>
  );
}
