import { createSignal } from "solid-js";
import { mount, ColorThemeProvider, ComponentsProvider, View, Text, Button, Switch, Checkbox } from "@wabou/ui";
import "virtual:wabou-stylesheet";
function Gallery() {
  const [count, setCount] = createSignal(0);
  const [notifications, setNotifications] = createSignal(false);
  const [autosave, setAutosave] = createSignal(false);
  const [dark, setDark] = createSignal(false);
  const [large, setLarge] = createSignal(false);
  return <ColorThemeProvider theme={dark() ? "dark" : "light"}><ComponentsProvider theme={dark() ? "dark" : "light"}>
    <View class="w-full h-full flex flex-col gap-5 p-8 bg-canvas text-primary">
      <Text class="text-[13px] text-accent">PLICO / WABOU</Text>
      <Text class="text-[30px]">Component gallery</Text>
      <Text class="text-[15px]">Solid components · retained Wabou backend</Text>
      <View class="flex flex-row gap-4 items-center">
        <View class="w-40"><Switch label="Dark theme" checked={dark()} onCheckedChange={setDark} /></View>
        <View class="w-40"><Checkbox label="Large text" checked={large()} onCheckedChange={setLarge} /></View>
      </View>
      <View class="flex flex-col gap-3">
        <Text class="text-xl">01   Buttons</Text>
        <View class="flex flex-row gap-3">
          <Button onClick={() => setCount(n => n+1)}>Increment</Button>
          <Button variant="secondary" onClick={() => setCount(0)}>Reset</Button>
          <Button disabled>Disabled</Button>
        </View>
        <Text class={large() ? "text-[22px]" : "text-base"}>Clicked {count()} times</Text>
      </View>
      <View class="flex flex-col gap-3">
        <Text class="text-xl">02   Controlled toggles</Text>
        <Switch label="Notifications" checked={notifications()} onCheckedChange={setNotifications} />
        <Checkbox label="Save automatically" checked={autosave()} onCheckedChange={setAutosave} />
        <Text>{notifications() ? "Notifications are enabled" : "Notifications are paused"}</Text>
      </View>
      <View class="flex flex-col gap-2">
        <Text class="text-xl">03   Reactive preview</Text>
        <Text class={large() ? "text-2xl" : "text-base"}>The quick brown fox jumps over the lazy dog.</Text>
        <Text>{autosave() ? "Changes will be saved automatically." : "Automatic saving is off."}</Text>
      </View>
    </View>
  </ComponentsProvider></ColorThemeProvider>;
}
mount(() => <Gallery />);
