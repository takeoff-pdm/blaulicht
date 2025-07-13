<script lang="ts">
 import Cable from "@lucide/svelte/icons/cable";
 import MonitorCog from "@lucide/svelte/icons/monitor-cog";

 import * as Sidebar from "$lib/components/ui/sidebar/index.js";
 
 // Menu items.
 const items = [
  {
   title: "Home",
   url: "/",
   icon: MonitorCog,
  },
  {
   title: "Fixtures",
   url: "/fixtures",
   icon: Cable,
  },
 ];
</script>
 
<Sidebar.Root>
 <Sidebar.Content>
  <Sidebar.Group>
   <Sidebar.GroupLabel>Application</Sidebar.GroupLabel>
   <Sidebar.GroupContent>
    <Sidebar.Menu>
     {#each items as item (item.title)}
      <Sidebar.MenuItem>
       <Sidebar.MenuButton>
        {#snippet child({ props })}
         <a href={item.url} {...props}>
          <item.icon />
          <span>{item.title}</span>
         </a>
        {/snippet}
       </Sidebar.MenuButton>
      </Sidebar.MenuItem>
     {/each}
    </Sidebar.Menu>
   </Sidebar.GroupContent>
  </Sidebar.Group>
 </Sidebar.Content>
</Sidebar.Root>