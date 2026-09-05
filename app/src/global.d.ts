interface StickiesBootstrap {
  folder?: string;
  label?: string;
  corner?: boolean;
}

interface Window {
  __STICKIES__?: StickiesBootstrap;
}
