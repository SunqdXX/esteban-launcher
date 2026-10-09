import { cx } from "../cx";
import styles from "./ModeSwitch.module.css";

interface ModeSwitchProps {
  hacked: boolean;
  disabled?: boolean;
  compact?: boolean;
  onChange: (hacked: boolean) => void;
}

const MODES = [
  { hacked: false, label: "Normal" },
  { hacked: true, label: "Hacked" },
];

export default function ModeSwitch({ hacked, disabled, compact, onChange }: ModeSwitchProps) {
  return (
    <div className={cx(styles.modes, compact && styles.compact)} role="radiogroup" aria-label="Mode">
      {MODES.map((mode) => (
        <button
          key={mode.label}
          type="button"
          role="radio"
          aria-checked={mode.hacked === hacked}
          disabled={disabled}
          className={cx(styles.mode, mode.hacked === hacked && styles.on, mode.hacked && styles.hacked)}
          onClick={() => {
            if (mode.hacked !== hacked) onChange(mode.hacked);
          }}
        >
          <span className={styles.dot} />
          {mode.label}
        </button>
      ))}
    </div>
  );
}
