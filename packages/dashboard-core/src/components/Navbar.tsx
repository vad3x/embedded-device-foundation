import { Moon, Sun } from 'lucide-solid';
import { type Component, createEffect, createSignal, type JSX } from 'solid-js';
import { themeFromStorage } from '../presentation';
import type { SystemDiagnosticsBase } from '../types';
import { Badge } from './Badge';

export interface NavbarProps {
  title: string;
  brandIcon: JSX.Element;
  badges: JSX.Element;
  status?: SystemDiagnosticsBase | null;
  themeStorageKey: string;
}

export const Navbar: Component<NavbarProps> = (props) => {
  const [theme, setTheme] = createSignal<'night' | 'light'>('night');
  const ecosystem = () => props.status?.active_ecosystem;
  const ecosystemName = () => {
    const type = ecosystem()?.type;
    return type ? `${type.charAt(0).toUpperCase()}${type.slice(1)}` : 'Matter';
  };
  const ecosystemVariant = () => {
    if (!props.status) return 'neutral';
    return ecosystem()?.paired ? 'success' : 'warning';
  };
  const dotClass = () => {
    if (!props.status) return 'bg-base-content/30 animate-pulse';
    return ecosystem()?.paired ? 'bg-success' : 'bg-warning';
  };

  createEffect(() => {
    const key = props.themeStorageKey;
    let next: 'night' | 'light' = 'night';
    try {
      next = themeFromStorage(localStorage, key);
    } catch {
      // Storage can be unavailable in embedded browsers.
    }
    setTheme(next);
    document.documentElement.setAttribute('data-theme', next);
  });

  const toggleTheme = () => {
    const next = theme() === 'night' ? 'light' : 'night';
    setTheme(next);
    document.documentElement.setAttribute('data-theme', next);
    try {
      localStorage.setItem(props.themeStorageKey, next);
    } catch {
      // Theme stays active for the session if storage is restricted.
    }
  };

  return (
    <header class="navbar bg-base-200 border-b border-base-content/10 px-4 sm:px-6 justify-between flex-wrap gap-3">
      <div class="flex items-center gap-3">
        <div class="p-2 rounded-lg bg-primary/15 text-primary">{props.brandIcon}</div>
        <span class="font-bold text-lg tracking-tight text-base-content">{props.title}</span>
      </div>
      <div class="flex items-center flex-wrap gap-2.5">
        {props.badges}
        <Badge variant={ecosystemVariant()}>
          <span class={`w-2 h-2 rounded-full ${dotClass()}`} />
          <span class="lowercase">ecosystem:</span>
          <span class="normal-case">{ecosystemName()}</span>
        </Badge>
        <button
          type="button"
          class="btn btn-ghost btn-circle btn-sm text-base-content/70 hover:text-base-content ml-1"
          onClick={toggleTheme}
          aria-label={theme() === 'night' ? 'Switch to light theme' : 'Switch to dark theme'}
          title={theme() === 'night' ? 'Switch to light mode' : 'Switch to dark mode'}
        >
          {theme() === 'night' ? (
            <Sun class="w-4 h-4 text-warning" />
          ) : (
            <Moon class="w-4 h-4 text-primary" />
          )}
        </button>
      </div>
    </header>
  );
};
