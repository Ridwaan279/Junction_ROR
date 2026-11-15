"use client";
import Link from "next/link";
import s from "./ops.module.css";

export type Locale = "pt" | "en";
/** English-only interface; legacy stored language preferences are ignored. */
export function useLocale(): [Locale] {
  return ["en"];
}

const LABELS = { pt: { map: "Mapa", data: "Dados", methods: "Método", recon: "Reconhecimento" }, en: { map: "Map", data: "Data", methods: "Methods", recon: "Recon" } };

export default function SiteNav({ active, locale }: { active: "map" | "data" | "methods" | "recon"; locale: Locale }) {
  const L = LABELS[locale];
  return (
    <header className={s.siteHeader}>
      <Link href="/" className={s.wordmark}>EmberWatch</Link>
      <nav className={s.tabs} aria-label="EmberWatch">
        {(["map", "data", "methods", "recon"] as const).map((k) => (
          <Link key={k} href={k === "map" ? "/" : "/" + k} aria-current={active === k ? "page" : undefined}>{L[k]}</Link>
        ))}
      </nav>
    </header>
  );
}
