import { useEffect, useRef, useState } from "react";
import { cx } from "../cx";
import PixelIcon, { CHEVRON_DOWN, CHEVRON_UP } from "./PixelIcon";
import styles from "./VersionPicker.module.css";

export interface DropdownOption {
  value: string;
  label: string;
  note: string | null;
}

interface DropdownProps {
  options: DropdownOption[];
  value: string;
  label: string;
  disabled?: boolean;
  onChange: (value: string) => void;
}

export default function Dropdown({ options, value, label, disabled, onChange }: DropdownProps) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const current = options.find((o) => o.value === value);

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
    <div ref={root} className={cx(styles.picker, styles.compact)}>
      {open && (
        <div className={cx(styles.list, styles.down)}>
          <div className={styles.scroll}>
            <ul className={styles.group} role="listbox" aria-label={label}>
              {options.map((o) => (
                <li key={o.value}>
                  <button
                    type="button"
                    role="option"
                    aria-selected={o.value === value}
                    className={cx(styles.option, o.value === value && styles.on)}
                    onClick={() => {
                      setOpen(false);
                      if (o.value !== value) onChange(o.value);
                    }}
                  >
                    <span className={styles.v}>{o.label}</span>
                    {o.note && <span className={styles.tag}>{o.note}</span>}
                  </button>
                </li>
              ))}
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
        aria-label={label}
        onClick={() => {
          setOpen(!open);
        }}
      >
        <span className={styles.v}>{current?.label ?? value}</span>
        {current?.note && <span className={styles.selectTag}>{current.note}</span>}
        <span className={styles.chev}>
          <PixelIcon cells={open ? CHEVRON_UP : CHEVRON_DOWN} />
        </span>
      </button>
    </div>
  );
}
