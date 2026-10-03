import {
  Aperture, Archive, ArrowLeftRight, Braces, Briefcase, Camera, CardSim, Check, ChevronDown, ChevronsUpDown, CircleAlert,
  CircleCheck, CircleX, Clock, Cloud, Copy, Cpu, Drone, Eraser, Eye, File, Film, FingerprintPattern, Folder, FolderOpen,
  FolderPlus, Funnel, GitFork, Globe, HardDrive, Heart, House, Image, Images, Info, Laptop, LayoutGrid, Link, List,
  LoaderCircle, LogIn, LogOut, MapPin, MemoryStick, Monitor, Moon, Mountain, Network, Pause, Pencil, Plane, Play, Plug,
  Plus, RefreshCw, RotateCcw, Search, Server, Settings, Shield, ShieldAlert, ShieldCheck, ShieldX, SlidersHorizontal,
  Smartphone, Star, Sun, Tag, Trash, TriangleAlert, Unplug, Usb, Video, WifiOff, Workflow, X, Zap,
  type IconNode,
} from "lucide";

/** Only icons registered here are bundled. Keys are kebab-case Lucide names. */
export const ICONS = {
  aperture: Aperture, archive: Archive, "arrow-left-right": ArrowLeftRight, braces: Braces, briefcase: Briefcase,
  camera: Camera, "card-sim": CardSim, check: Check, "chevron-down": ChevronDown, "chevrons-up-down": ChevronsUpDown,
  "circle-alert": CircleAlert, "circle-check": CircleCheck, "circle-x": CircleX, clock: Clock, cloud: Cloud, copy: Copy,
  cpu: Cpu, drone: Drone, eraser: Eraser, eye: Eye, file: File, film: Film, fingerprint: FingerprintPattern,
  folder: Folder, "folder-open": FolderOpen, "folder-plus": FolderPlus, funnel: Funnel, "git-fork": GitFork, globe: Globe,
  "hard-drive": HardDrive, heart: Heart, house: House, image: Image, images: Images, info: Info, laptop: Laptop,
  "layout-grid": LayoutGrid, link: Link, list: List, loader: LoaderCircle, "log-in": LogIn, "log-out": LogOut,
  "map-pin": MapPin, "memory-stick": MemoryStick, monitor: Monitor, moon: Moon, mountain: Mountain, network: Network,
  pause: Pause, pencil: Pencil, plane: Plane, play: Play, plug: Plug, plus: Plus, "refresh-cw": RefreshCw,
  "rotate-ccw": RotateCcw, search: Search, server: Server, settings: Settings, shield: Shield, "shield-alert": ShieldAlert,
  "shield-check": ShieldCheck, "shield-x": ShieldX, sliders: SlidersHorizontal, smartphone: Smartphone, star: Star,
  sun: Sun, tag: Tag, trash: Trash, "triangle-alert": TriangleAlert, unplug: Unplug, usb: Usb, video: Video,
  "wifi-off": WifiOff, workflow: Workflow, x: X, zap: Zap,
} satisfies Record<string, IconNode>;

export type IconName = keyof typeof ICONS;

/** Icons offered when choosing a space icon. */
export const SPACE_ICONS: IconName[] = ["plane", "house", "archive", "briefcase", "camera", "mountain", "heart", "star", "globe", "cloud"];
