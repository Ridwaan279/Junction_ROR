export type IconName =
  | "flame"
  | "map"
  | "layers"
  | "plane"
  | "message"
  | "play"
  | "pause"
  | "reset"
  | "arrow"
  | "external"
  | "wind"
  | "pin"
  | "check"
  | "warning"
  | "target"
  | "close";
const paths: Record<IconName, React.ReactNode> = {
  flame: (
    <path d="M12 3c1 5-4 5-3 9 0 1 1 2 2 2-1-3 3-4 3-7 4 4 5 7 4 10a6.3 6.3 0 0 1-12-1C5 11 9 9 12 3Z" />
  ),
  map: (
    <>
      <path d="m3 6 6-3 6 3 6-3v15l-6 3-6-3-6 3Z" />
      <path d="M9 3v15M15 6v15" />
    </>
  ),
  layers: (
    <>
      <path d="m3 8 9-5 9 5-9 5Z" />
      <path d="m3 12 9 5 9-5M3 16l9 5 9-5" />
    </>
  ),
  plane: (
    <>
      <path d="m22 2-7 20-4-9-9-4Z" />
      <path d="m22 2-11 11" />
    </>
  ),
  message: <path d="M21 11a9 9 0 0 1-9 9H3l2-5a9 9 0 1 1 16-4Z" />,
  play: <path d="m8 5 11 7-11 7Z" />,
  pause: (
    <>
      <path d="M8 5v14M16 5v14" />
    </>
  ),
  reset: (
    <>
      <path d="M3 10a9 9 0 1 1 1 8M3 4v6h6" />
    </>
  ),
  arrow: (
    <>
      <path d="m9 5 7 7-7 7" />
    </>
  ),
  external: (
    <>
      <path d="M14 3h7v7M21 3l-9 9" />
      <path d="M10 3H4v17h17v-6" />
    </>
  ),
  wind: (
    <>
      <path d="M3 8h12a3 3 0 1 0-3-3M3 12h15a3 3 0 1 1-3 3M3 16h5a3 3 0 1 1-3 3" />
    </>
  ),
  pin: (
    <>
      <path d="M20 10c0 6-8 12-8 12S4 16 4 10a8 8 0 1 1 16 0Z" />
      <circle cx="12" cy="10" r="2" />
    </>
  ),
  check: <path d="m5 12 4 4L20 5" />,
  warning: (
    <>
      <path d="m12 3 10 18H2Z" />
      <path d="M12 9v5M12 17h.01" />
    </>
  ),
  target: (
    <>
      <circle cx="12" cy="12" r="7" />
      <path d="M12 2v4M12 18v4M2 12h4M18 12h4" />
      <circle cx="12" cy="12" r="2" />
    </>
  ),
  close: <path d="m6 6 12 12M18 6 6 18" />,
};
export default function Icon({
  name,
  size = 20,
}: {
  name: IconName;
  size?: number;
}) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.7"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {paths[name]}
    </svg>
  );
}
