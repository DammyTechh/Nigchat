# NigChat — Design Notes

The brief was: mature green and white, real icons, light and dark, responsive on every
iOS and Android device, and **not** a clone of the obvious competitor despite having the
same features. This is how each of those was addressed.

---

## Not looking like the competitor

Feature parity does not require visual parity. Four decisions carry most of the
difference, and none of them cost a feature.

**1. Green is an accent, never a surface.** There is no green app bar, no green screen
background, no green splash. Chrome is white or near-black; green appears only on things
you can act on — the send button, the unread count, the active tab, your own messages.
This one rule is the single biggest source of visual distance, because a coloured top bar
is what people actually picture when they picture the competitor.

**2. Large-title headers, not centred bar titles.** "Chats" sits at 32pt on the page
background and scrolls with the content, closer to a modern OS settings app than to a
traditional chat client.

**3. A pill-shaped tab indicator.** The active tab gets a soft green pill behind the
icon rather than a tinted glyph. It is the app's signature in permanent chrome.

**4. Different bubble geometry.** 20pt radius with the tail corner tightened to 6, no
drawn tail triangle, and incoming bubbles filled with a hairline rather than white with a
drop shadow. Consecutive messages tighten to 8 at the join so a burst reads as one block
instead of a stack of identical lozenges. Bubble shape is where a messaging app is most
recognisable, so it is where the most deliberate work went.

Beyond those: the chat list uses a segmented filter (All / Unread / Groups) rather than
archive-and-swipe, and Updates is a horizontal rail of story rings above channel cards
rather than one long list of names.

---

## Translucency ("glass")

Blurred, translucent chrome — the tab bar, the composer, overlays on the camera —
using `expo-blur`. On iOS that maps straight onto `UIVisualEffectView`, the same
hardware-accelerated material the OS uses for its own bars.

**Android needs an honest answer, because this is where cross-platform apps go
wrong.** Android has no backdrop-blur primitive below API 31. Before Android 12
there is no `RenderEffect`, so any "blur" is a JS-side downscale-and-redraw that
drops frames the moment a list scrolls behind it. Shipping that would make the
app feel *worse* on exactly the devices most users have.

So `<Glass>` tiers its behaviour:

| Platform | Behaviour |
|---|---|
| iOS | Real `UIVisualEffectView` blur |
| Android 12+ (API 31) | `RenderEffect` blur via `dimezisBlurView` |
| Android < 12 | No blur — a near-opaque tinted surface instead |

The bottom tier still looks deliberate rather than broken, because **the design
never depends on blur for legibility**. Every glass surface carries a tint layer
and a hairline underneath; blur is a finish, not the structure. That is also why
text stays readable over a bright photo — Apple's own materials do the same
thing, since "frosted glass" is blur *plus* a translucent fill, never blur alone.

Three elevations, tuned separately per platform because the same numeric
intensity reads much heavier on Android:

- `chrome` — tab bar, headers. Subtle; content reads through.
- `panel` — the composer. Stronger; text sits on it.
- `overlay` — sheets, the scanner's instruction card. Heaviest.

Because the tab bar now floats over content, every tab screen adds matching
bottom clearance so the last row is never permanently hidden underneath it.

---

## Colour

The greens are **sampled from the logo file**, not guessed:

| Source in the artwork | Hex | Role |
|---|---|---|
| Deep side of the bubble gradient | `#0F663F` | pressed states, deep accents |
| Light side of the bubble | `#179759` | dark-mode primary |
| Bright "Chat" wordmark | `#22C55E` | read receipts, highlights |
| Chosen mid-tone | `#0E7A46` | light-mode primary |

Neutrals are very slightly green-shifted so they sit under the brand instead of reading
as two unrelated palettes.

### Dark mode is a design, not an inversion

Two decisions that would be wrong in a naive port:

- **Background is `#0B120E`, not pure black.** On OLED, pure black behind a scrolling
  list produces visible smearing, and every elevation step above it has to be faked with
  borders.
- **The green lifts to `#179759`.** The light-mode primary fails contrast on a dark
  surface. A brand colour that is unreadable at night is a bug, not a style.

Elevation is expressed with surface colour and hairlines rather than shadows, because
shadows disappear in dark mode. Shadows are reserved for things that genuinely float —
the FAB and sheets.

---

## Icons

[Lucide](https://lucide.dev) throughout: real vector icons on a consistent 24px grid with
a 2px stroke. No emoji standing in for icons, and no mixing sets — consistent optical
weight across every glyph is one of the quiet things that makes an interface feel
finished rather than assembled. Active tabs bump to 2.2 stroke, which reads as weight
rather than as a different icon.

---

## Type

System fonts: SF Pro on iOS, Roboto on Android. A custom face would add a download, a
flash of unstyled text, and would look subtly wrong next to the platform's own keyboard
and share sheet. Messaging apps live inside the OS and should read like it.

Every string goes through one `<Text>` component with a named variant, so the scale
cannot drift. `allowFontScaling` stays on to honour the OS text-size setting, but is
capped at 1.6× — beyond that an unbounded chat row grows tall enough to push the
timestamp off screen.

---

## Responsiveness

**Safe areas are read, never hardcoded.** `react-native-safe-area-context` reports the
real insets, so the layout is correct on an iPhone SE (no inset), a notched device, and
Dynamic Island models — and a future device shape needs no change here.

**Content clamps at 720pt and centres.** This is what makes the app work on an iPad or an
unfolded foldable without a separate tablet build. A chat list stretched across a 12"
display is unreadable, and stretching is what most phone-first apps do when they meet a
large screen.

**Small phones are handled explicitly.** The welcome screen drops to two highlights below
700pt of height so the primary button never falls below the fold.

**The keyboard is handled per platform.** `KeyboardAvoidingView` with `padding` on iOS,
Android's own `softwareKeyboardLayoutMode: pan`; the message list is inverted so it stays
pinned to the newest message with no scroll maths.

**Tap targets are never below 44pt**, and icon-only buttons carry 8pt of `hitSlop`
because they are frequently drawn smaller than they are comfortable to hit.

---

## Assets

Every icon and splash is generated from the supplied logo, fitted rather than stretched:

| File | Size | Fitting |
|---|---|---|
| `icon.png` | 1024² | Mark only, 14% padding — iOS clips to a squircle, so the bubble's tail must clear the corner radius |
| `adaptive-icon.png` | 1024² | 26% padding — Android launchers mask up to a third of the edge |
| `splash-light.png` | 1284² | Full lockup, generous margin |
| `splash-dark.png` | 1284² | Full lockup on `#0B120E` — the app's own dark background, so launch hands off to the first screen with no colour jump |
| `logo-full-dark.png` | — | Wordmark recoloured for dark surfaces |
| `notification-icon.png` | 96² | Flat white silhouette — Android renders notification icons as a mask, so a coloured mark would appear as a white blob |

The splash carries through to the app: the launch screen and the welcome screen show the
same lockup at the same optical size, so there is no jump at handoff.

**Extracting the mark took three attempts, and the reasons are worth recording:**

1. The supplied logo is RGB with no transparency, so a naive crop bakes a white box in.
   On the dark splash that rendered as a white rectangle behind the mark.
2. Flood-filling every white pixel erases the "N", which is itself white. Filling only
   from the image edges preserves white enclosed by the green.
3. Brightness alone cannot separate backdrop from artwork — the highlight inside the "N"
   is nearly white — so the fill walked through it and ate the speech-bubble tail. The
   distinguishing property is *neutrality*: the backdrop is pure grey-white, the logo's
   lightest pixels keep a green cast.
4. Recolouring the dark wordmark by colour repaints the bubble too: `#0B3B2A` (the "Nig"
   ink) and `#0F663F` (the bubble's deepest gradient) are both dark green. The recolour is
   therefore restricted to the wordmark band, found by detecting the transparent rows
   between mark and text.

Edges use partial alpha rather than a hard cut, so nothing shows a grey fringe on a dark
background.

---

## Details worth keeping

- **Unread rows put the timestamp in brand green.** The eye finds the new conversation
  without reading a word.
- **The send button only turns solid green when there is something to send.** The
  affordance appears exactly when it means something; before that the slot holds the mic.
- **Delivery ticks change colour, not shape, between delivered and read.** Glanceable
  without comparing two near-identical glyphs.
- **The appearance screen previews real bubbles.** Theme choices are hard to judge from a
  swatch.
- **Connection state is a banner in the list, not a toast.** It stays put and explains
  why nothing is arriving.
- **Empty states everywhere.** A blank screen reads as a bug.
- **Avatar tiles use six muted greens and neutrals**, deterministic from the name. A wall
  of saturated rainbow circles is the fastest way to make a list look cheap.
