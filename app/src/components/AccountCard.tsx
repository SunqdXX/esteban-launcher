import styles from "./AccountCard.module.css";

const HEAD = [
  "..####..",
  ".#....#.",
  "......#.",
  ".....#..",
  "....#...",
  "....#...",
  "........",
  "....#...",
];

export default function AccountCard() {
  return (
    <div className={styles.card}>
      <div className={styles.head} aria-hidden="true">
        {HEAD.join("")
          .split("")
          .map((cell, i) => (
            <i key={i} className={cell === "#" ? styles.on : undefined} />
          ))}
      </div>
      <div className={styles.who}>
        <div className={styles.name}>Not signed in</div>
        <div className={styles.hint}>Microsoft sign-in comes in a later update</div>
      </div>
      <button type="button" className={styles.button} disabled>
        Sign in
      </button>
    </div>
  );
}
