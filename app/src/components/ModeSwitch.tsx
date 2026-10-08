import { MODE_LABEL, type Profile } from "../api";
import { cx } from "../cx";
import styles from "./ModeSwitch.module.css";

interface ModeSwitchProps {
  value: Profile;
  disabled?: boolean;
  compact?: boolean;
  onChange: (profile: Profile) => void;
}

const MODES: Profile[] = ["clean", "hacks"];

export default function ModeSwitch({ value, disabled, compact, onChange }: ModeSwitchProps) {
  return (
    <div className={cx(styles.modes, compact && styles.compact)} role="radiogroup" aria-label="Mode">
      {MODES.map((mode) => (
        <button
          key={mode}
          type="button"
          role="radio"
          aria-checked={mode === value}
          disabled={disabled}
          className={cx(styles.mode, mode === value && styles.on, mode === "hacks" && styles.hacked)}
          onClick={() => {
            if (mode !== value) onChange(mode);
          }}
        >
          <span className={styles.dot} />
          {MODE_LABEL[mode]}
        </button>
      ))}
    </div>
  );
}
