import { cx } from "../cx";
import styles from "./Toggle.module.css";

interface ToggleProps {
  on: boolean;
  label: string;
  disabled?: boolean;
  onChange: (on: boolean) => void;
}

export default function Toggle({ on, label, disabled, onChange }: ToggleProps) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      aria-label={label}
      disabled={disabled}
      className={cx(styles.toggle, on && styles.on)}
      onClick={() => {
        onChange(!on);
      }}
    >
      <span className={styles.knob} />
    </button>
  );
}
