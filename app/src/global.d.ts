interface StickiesBootstrap {
  folder?: string;
  label?: string;
  corner?: boolean;
  options?: boolean;
}

interface Window {
  __STICKIES__?: StickiesBootstrap;
}
