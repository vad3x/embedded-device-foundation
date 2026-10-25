import type { Component, ComponentProps } from 'solid-js';
import { splitProps } from 'solid-js';
import { type CardBackground, cardBackgroundClass } from '../presentation';

export interface CardProps extends ComponentProps<'div'> {
  bordered?: boolean;
  compact?: boolean;
  bg?: CardBackground;
}

export const Card: Component<CardProps> = (props) => {
  const [local, rest] = splitProps(props, ['bordered', 'compact', 'bg', 'class', 'children']);
  const classes = () =>
    [
      'card',
      cardBackgroundClass(local.bg),
      local.bordered ? 'card-border border-base-content/10' : '',
      local.compact ? 'card-sm' : '',
      'shadow-xs',
      local.class ?? '',
    ]
      .filter(Boolean)
      .join(' ');
  return (
    <div class={classes()} {...rest}>
      {local.children}
    </div>
  );
};

export const CardBody: Component<ComponentProps<'div'>> = (props) => {
  const [local, rest] = splitProps(props, ['class', 'children']);
  return (
    <div class={`card-body p-4 sm:p-5 ${local.class ?? ''}`} {...rest}>
      {local.children}
    </div>
  );
};

export const CardTitle: Component<ComponentProps<'h3'>> = (props) => {
  const [local, rest] = splitProps(props, ['class', 'children']);
  return (
    <h3
      class={`card-title text-sm font-semibold tracking-wider uppercase text-base-content/60 ${local.class ?? ''}`}
      {...rest}
    >
      {local.children}
    </h3>
  );
};

export const CardActions: Component<ComponentProps<'div'>> = (props) => {
  const [local, rest] = splitProps(props, ['class', 'children']);
  return (
    <div class={`card-actions justify-end mt-4 ${local.class ?? ''}`} {...rest}>
      {local.children}
    </div>
  );
};
