import type { Component, ComponentProps } from 'solid-js';
import type { BadgeVariant } from '../presentation';

export interface BadgeProps extends ComponentProps<'span'> {
  variant?: BadgeVariant;
  size?: 'xs' | 'sm' | 'md';
}

const variantClasses: Record<BadgeVariant, string> = {
  success: 'bg-success/15 text-success border-success/30',
  warning: 'bg-warning/15 text-warning border-warning/40',
  error: 'bg-error/15 text-error border-error/30',
  info: 'bg-info/15 text-info border-info/30',
  ghost: 'bg-base-content/5 text-base-content/70 border-base-content/15',
  neutral: 'bg-base-content/10 text-base-content/80 border-base-content/20',
};
const sizeClasses = {
  xs: 'px-2 py-0.5 text-[10px]',
  sm: 'px-2.5 py-0.5 text-xs',
  md: 'px-3 py-1 text-xs',
};

export const Badge: Component<BadgeProps> = (props) => {
  const variant = () => props.variant ?? 'neutral';
  const size = () => props.size ?? 'sm';
  const classes = () =>
    [
      'inline-flex items-center gap-1.5 font-semibold uppercase tracking-wider rounded-full border shadow-sm transition-colors',
      variantClasses[variant()],
      sizeClasses[size()],
      props.class ?? '',
    ]
      .filter(Boolean)
      .join(' ');
  return <span class={classes()}>{props.children}</span>;
};
