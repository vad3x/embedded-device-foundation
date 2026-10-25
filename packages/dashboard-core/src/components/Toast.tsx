import type { Component } from 'solid-js';
import { Show } from 'solid-js';

export interface ToastProps {
  message: string | null;
  type?: 'info' | 'success' | 'error';
}

const alertClasses = {
  info: 'alert-info',
  success: 'alert-success',
  error: 'alert-error',
};

export const Toast: Component<ToastProps> = (props) => (
  <Show when={props.message}>
    <div class="toast toast-end toast-bottom z-50">
      <div
        class={`alert py-2 px-4 shadow-lg text-sm font-medium ${alertClasses[props.type ?? 'info']}`}
      >
        <span>{props.message}</span>
      </div>
    </div>
  </Show>
);
