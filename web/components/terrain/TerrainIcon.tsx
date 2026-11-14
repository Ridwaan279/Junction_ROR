type Name = "mountain" | "city" | "search" | "plus" | "minus" | "compass" | "share" | "chevron" | "rotate" | "expand" | "contour" | "back" | "pin" | "close" | "globe";
const paths: Record<Name, React.ReactNode> = {
  city: <><path d="M2 21h20M4 21V9h7v12m0-16h8v16M6 12h2m-2 4h2m6-8h2m-2 4h2m-2 4h2" /></>,
  mountain: <><path d="m2 20 7-14 4 7 3-5 6 12Z"/><path d="m6 12 3 2 2-3m3 3 2 1 2-2"/></>,
  search: <><circle cx="10.5" cy="10.5" r="6.5"/><path d="m16 16 5 5"/></>,
  plus: <path d="M12 5v14M5 12h14"/>,
  minus: <path d="M5 12h14"/>,
  compass: <><circle cx="12" cy="12" r="9"/><path d="m15.5 8.5-2 5-5 2 2-5Z"/></>,
  share: <><path d="M12 16V3m-5 5 5-5 5 5M5 13v7h14v-7"/></>,
  chevron: <path d="m8 5 7 7-7 7"/>,
  rotate: <><path d="M21 7v6h-6M3 17v-6h6"/><path d="M5.5 6a8 8 0 0 1 14 2M4.5 16a8 8 0 0 0 14 2"/></>,
  expand: <path d="M9 3H3v6m12-6h6v6M3 15v6h6m6 0h6v-6"/>,
  contour: <><path d="M2 17c2-3 3 1 6-3s4-1 6-5 6-5 8-3M2 21c4-3 5 1 8-3s5 0 8-4 2-2 4-3M2 11c3-2 4 0 6-3s4-5 7-6"/><path d="M14 22c2-2 4-1 5-3s2-2 3-2"/></>,
  back: <path d="m10 5-7 7 7 7M3 12h18"/>,
  pin: <><path d="M20 10c0 6-8 12-8 12S4 16 4 10a8 8 0 1 1 16 0Z"/><circle cx="12" cy="10" r="2.5"/></>,
  close: <path d="m6 6 12 12M18 6 6 18"/>,
  globe: <><circle cx="12" cy="12" r="9"/><ellipse cx="12" cy="12" rx="4" ry="9"/><path d="M3 12h18"/></>,
};
export default function TerrainIcon({ name, size = 20 }: { name: Name; size?: number }) {
  return <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">{paths[name]}</svg>;
}
