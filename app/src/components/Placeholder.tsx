import logoUrl from "../assets/logo/EstebanLogo.png";
import styles from "./Placeholder.module.css";

interface PlaceholderProps {
  title: string;
  showLogo: boolean;
}

export default function Placeholder({ title, showLogo }: PlaceholderProps) {
  return (
    <section className={styles.wrap}>
      <h1 className={styles.title}>{title}</h1>
      <p className={styles.note}>Not built yet. This screen lands in M4.</p>
      {showLogo && <img className={styles.logo} src={logoUrl} alt="Esteban" width={1408} height={768} />}
    </section>
  );
}
