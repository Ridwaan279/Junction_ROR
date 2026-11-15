"use client";
import Link from "next/link";
import { useEffect, useState } from "react";
import s from "./ops.module.css";

export type Locale = "pt" | "en";
const KEY = "emberwatch-locale";

/** Language shared across tabs; storage is a convenience only (falls back to PT). */
export function useLocale(): [Locale, (l: Locale) => void] {
  const [locale, setLocale] = useState<Locale>("pt");
  useEffect(() => {
    try {
      const saved = window.localStorage.getItem(KEY);
      if (saved === "en" || saved === "pt") setTimeout(() => setLocale(saved), 0);
    } catch { /* storage unavailable */ }
  }, []);
  useEffect(() => { document.documentElement.lang = locale; }, [locale]);
  const set = (l: Locale) => {
    setLocale(l);
    try { window.localStorage.setItem(KEY, l); } catch { /* storage unavailable */ }
  };
  return [locale, set];
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
