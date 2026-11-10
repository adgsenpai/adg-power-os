// Small stroke icon set (24px grid, currentColor) so the bundle needs no icon library.
import type { SVGProps } from "react";

type P = SVGProps<SVGSVGElement> & { size?: number };

function base({ size = 22, ...rest }: P, children: React.ReactNode) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.8}
      strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" {...rest}>
      {children}
    </svg>
  );
}

export const Grid = (p: P) => base(p, <><rect x="3" y="3" width="7" height="7" rx="1.5" /><rect x="14" y="3" width="7" height="7" rx="1.5" /><rect x="3" y="14" width="7" height="7" rx="1.5" /><rect x="14" y="14" width="7" height="7" rx="1.5" /></>);
export const Terminal = (p: P) => base(p, <><rect x="2.5" y="4" width="19" height="16" rx="2.5" /><path d="m7 9 3 3-3 3M12.5 15H17" /></>);
export const Globe = (p: P) => base(p, <><circle cx="12" cy="12" r="9" /><path d="M3 12h18M12 3a14 14 0 0 1 0 18M12 3a14 14 0 0 0 0 18" /></>);
export const Folder = (p: P) => base(p, <path d="M3 7.5A2.5 2.5 0 0 1 5.5 5h3.6l2 2.2h7.4A2.5 2.5 0 0 1 21 9.7v7.8a2.5 2.5 0 0 1-2.5 2.5h-13A2.5 2.5 0 0 1 3 17.5z" />);
export const Music = (p: P) => base(p, <><path d="M9 18V5l11-2v13" /><circle cx="6" cy="18" r="3" /><circle cx="17" cy="16" r="3" /></>);
export const Wifi = (p: P & { level?: number }) => {
  const l = p.level ?? 4;
  const o = (n: number) => (l >= n ? 1 : 0.25);
  return base(p, <><path d="M2 8.8a15 15 0 0 1 20 0" opacity={o(4)} /><path d="M5 12.4a10 10 0 0 1 14 0" opacity={o(3)} /><path d="M8.5 15.9a5 5 0 0 1 7 0" opacity={o(2)} /><circle cx="12" cy="19.5" r="1" fill="currentColor" opacity={o(1)} /></>);
};
export const WifiOff = (p: P) => base(p, <><path d="m2 2 20 20M8.5 15.9a5 5 0 0 1 7 0M5 12.4a10 10 0 0 1 4.2-2.4M2 8.8a15 15 0 0 1 4.6-2.9M14.8 10.1A10 10 0 0 1 19 12.4M10.7 5.1A15 15 0 0 1 22 8.8" /><circle cx="12" cy="19.5" r="1" fill="currentColor" /></>);
export const Bluetooth = (p: P) => base(p, <path d="m7 7 10 10-5 5V2l5 5L7 17" />);
export const Speaker = (p: P) => base(p, <><path d="M11 5 6 9H3v6h3l5 4z" /><path d="M15.5 8.5a5 5 0 0 1 0 7M18.5 5.5a9 9 0 0 1 0 13" /></>);
export const Settings = (p: P) => base(p, <><circle cx="12" cy="12" r="3" /><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z" /></>);
export const Leaf = (p: P) => base(p, <><path d="M11 20A7 7 0 0 1 9.8 6.1C15.5 5 17 4.5 19 2c1 2 2 4.2 2 8 0 5.5-4.8 10-10 10Z" /><path d="M2 21c0-3 1.9-5.4 5.2-6.1A15 15 0 0 1 13 12" /></>);
export const Bolt = (p: P) => base(p, <path d="M13 2 4 14h7l-1 8 9-12h-7z" />);
export const Chip = (p: P) => base(p, <><rect x="5" y="5" width="14" height="14" rx="2" /><rect x="9" y="9" width="6" height="6" rx="1" /><path d="M9 2v3M15 2v3M9 19v3M15 19v3M2 9h3M2 15h3M19 9h3M19 15h3" /></>);
export const Sun = (p: P) => base(p, <><circle cx="12" cy="12" r="4" /><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4" /></>);
export const Monitor = (p: P) => base(p, <><rect x="2.5" y="3.5" width="19" height="13" rx="2" /><path d="M8 21h8M12 16.5V21" /></>);
export const Keyboard = (p: P) => base(p, <><rect x="2" y="6" width="20" height="12" rx="2" /><path d="M6 10h.01M10 10h.01M14 10h.01M18 10h.01M7 14h10" /></>);
export const Gpu = (p: P) => base(p, <><rect x="2" y="7" width="20" height="10" rx="2" /><circle cx="9" cy="12" r="2.5" /><circle cx="16" cy="12" r="2.5" /><path d="M5 17v3M2 10H1" /></>);
export const Shield = (p: P) => base(p, <><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10Z" /><path d="m9 12 2 2 4-4" /></>);
export const Power = (p: P) => base(p, <><path d="M12 2v10" /><path d="M18.4 6.6a9 9 0 1 1-12.8 0" /></>);
export const Lock = (p: P) => base(p, <><rect x="4" y="11" width="16" height="10" rx="2" /><path d="M8 11V7a4 4 0 0 1 8 0v4" /></>);
export const Refresh = (p: P) => base(p, <><path d="M21 12a9 9 0 0 1-15.5 6.2L3 16" /><path d="M3 12a9 9 0 0 1 15.5-6.2L21 8" /><path d="M21 3v5h-5M3 21v-5h5" /></>);
export const Chevron = (p: P) => base(p, <path d="m6 9 6 6 6-6" />);
export const Headphones = (p: P) => base(p, <><path d="M3 18v-6a9 9 0 0 1 18 0v6" /><path d="M21 19a2 2 0 0 1-2 2h-1v-6h3zM3 19a2 2 0 0 0 2 2h1v-6H3z" /></>);
export const Mouse = (p: P) => base(p, <><rect x="6" y="2" width="12" height="20" rx="6" /><path d="M12 6v4" /></>);
export const Play = (p: P) => base(p, <path d="M7 4v16l13-8z" />);
export const Battery = ({ level = 100, charging = false, ...p }: P & { level?: number; charging?: boolean }) =>
  base(p, <>
    <rect x="2" y="7" width="17" height="10" rx="2.5" />
    <path d="M22 11v2" />
    <rect x="4" y="9" width={Math.max(0.5, (13 * Math.min(100, Math.max(0, level))) / 100)} height="6" rx="1" fill="currentColor" stroke="none" />
    {charging && <path d="m11.5 8-2 4h3l-2 4" stroke="var(--color-bg)" strokeWidth={1.6} />}
  </>);
