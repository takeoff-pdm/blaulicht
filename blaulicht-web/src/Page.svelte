<script lang="ts">
    import { ThemeUtils } from 'svelte-tweakpane-ui';
    import { createSnackbar, fetchUserData, modifyUserData, user, loading } from './global'
    import { onMount } from 'svelte'
    // import Progress from './components/Progress.svelte'

    export let shouldFetchUserData = true

    export let pageId: 'dash'

    //document.documentElement.style.setProperty('--clr-primary-dark', '#ff0000')
    /* document.documentElement.style.setProperty(
        '--clr-on-primary-dark',
        contrast('#ff0000') === 'black' ? '#121212' : '#ffffff',
    ) */

    // let kitchen: KitchenComponentDev
    // $createSnackbar = (message: string, actions?: ConfigAction[]) => {
    //     kitchen.push({
    //         label: message,
    //         dismissButton: true,
    //         actions,
    //     })
    // }

    interface PageEntry {
        id: string
        href: string
        title: string
        icon: string
        isAdmin: boolean
    }

    const rawPages: PageEntry[] = [
        {
            id: 'dash',
            href: '/dash',
            title: 'Dashboard',
            icon: 'home',
            isAdmin: false,
        },
    ]

    let pages: PageEntry[] = []

    // $: if (!$user.isAdmin) {
    //     pages = rawPages.filter(p => !p.isAdmin)
    // }


    onMount(async () => {
        // ThemeUtils.setGlobalDefaultTheme(ThemeUtils.presets.retro);
        // if (shouldFetchUserData) {
        //     $loading = true
        //     await fetchUserData()
        //     $loading = false
        // }
        pages = rawPages
    })
</script>

<main class="main-content">
    <slot />
</main>

<style lang="scss">
    @use 'mixins' as *;

    /* :global(body) { */
    /*     margin: 0; */
    /*     height: 100vh; */
    /**/
    /*     @include mobile { */
    /*         height: auto; */
    /*     } */
    /* } */

    .main-content {
        height: 100vh;
    }

    .link {
        color: var(--clr-on-primary-dark);
    }

    .title {
        display: flex;
        align-items: center;
        gap: 0.2rem;
        padding-left: 0.3rem;

        span {
            overflow-x: hidden;
            white-space: nowrap;
            text-overflow: ellipsis;
        }
    }

    .subtitle {
        padding-left: 0.3rem;
    }

    .drawer-items {
        display: flex;
        flex-direction: column;
        justify-content: space-between;
        padding: 1rem 0;

        @include not-widescreen {
            height: calc(100vh - 14rem);
        }
    }
</style>
