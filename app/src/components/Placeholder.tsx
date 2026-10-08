import styles from "./Placeholder.module.css";

interface PlaceholderProps {
  title: string;
  note: string;
}

export default function Placeholder({ title, note }: PlaceholderProps) {
  return (
    <section className={styles.wrap}>
      <h1 className={styles.title}>{title}</h1>
      <p className={styles.note}>{note}</p>
    </section>
  );
}
