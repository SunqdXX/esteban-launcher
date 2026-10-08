import styles from "./Placeholder.module.css";

interface PlaceholderProps {
  title: string;
}

export default function Placeholder({ title }: PlaceholderProps) {
  return (
    <section className={styles.wrap}>
      <h1 className={styles.title}>{title}</h1>
      <p className={styles.note}>Not built yet. This screen comes in the next part of M4.</p>
    </section>
  );
}
