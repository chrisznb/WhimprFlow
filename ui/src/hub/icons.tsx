// Central icon set — Font Awesome, mapped behind a stable name -> icon API so
// call sites stay unchanged. Regular (outline) variants where they exist, to
// keep the light Wispr-style look; solid elsewhere.
import type { CSSProperties } from "react";
import { FontAwesomeIcon } from "@fortawesome/react-fontawesome";
import type { IconDefinition } from "@fortawesome/fontawesome-svg-core";
import {
  faHouse,
  faChartSimple,
  faScissors,
  faFont,
  faWandMagicSparkles,
  faGear,
  faMagnifyingGlass,
  faArrowDownWideShort,
  faPlus,
  faXmark,
  faMicrophone,
  faFire,
  faPaperPlane,
  faCommentDots,
  faFileAudio,
} from "@fortawesome/free-solid-svg-icons";
import {
  faBookOpen as faBookOpenSolid,
} from "@fortawesome/free-solid-svg-icons";
import {
  faPenToSquare,
  faCircleQuestion,
  faCopy,
} from "@fortawesome/free-regular-svg-icons";

export type IconName =
  | "home"
  | "insights"
  | "dictionary"
  | "snippets"
  | "style"
  | "transforms"
  | "scratchpad"
  | "settings"
  | "help"
  | "search"
  | "sort"
  | "plus"
  | "close"
  | "mic"
  | "flame"
  | "send"
  | "assistant"
  | "fileaudio"
  | "copy";

const ICONS: Record<IconName, IconDefinition> = {
  home: faHouse,
  insights: faChartSimple,
  dictionary: faBookOpenSolid,
  snippets: faScissors,
  style: faFont,
  transforms: faWandMagicSparkles,
  scratchpad: faPenToSquare,
  settings: faGear,
  help: faCircleQuestion,
  search: faMagnifyingGlass,
  sort: faArrowDownWideShort,
  plus: faPlus,
  close: faXmark,
  mic: faMicrophone,
  flame: faFire,
  send: faPaperPlane,
  assistant: faCommentDots,
  fileaudio: faFileAudio,
  copy: faCopy,
};

export function Icon({
  name,
  size = 18,
  strokeWidth: _strokeWidth,
  style,
}: {
  name: IconName;
  size?: number;
  /// Accepted for backwards compatibility; Font Awesome icons are filled paths.
  strokeWidth?: number;
  style?: CSSProperties;
}) {
  return (
    <FontAwesomeIcon
      icon={ICONS[name]}
      style={{ fontSize: size * 0.92, flex: "0 0 auto", ...style }}
      aria-hidden
    />
  );
}
