<script lang="ts">
    import { fetchSystems } from "../lib/api";
    import type { SystemEntry } from "../lib/api";

    let { channel, weeks }: { channel: string; weeks: number } = $props();

    let systems: SystemEntry[] = $state([]);
    let sortKey: keyof SystemEntry = $state("dirty_hours");
    let sortAsc: boolean = $state(false);

    $effect(() => {
        if (channel) {
            fetchSystems(channel, weeks).then((d) => (systems = d.systems));
        }
    });

    let sorted = $derived(
        [...systems].sort((a, b) => {
            const av = a[sortKey];
            const bv = b[sortKey];
            const cmp =
                typeof av === "string"
                    ? av.localeCompare(bv as string)
                    : (av as number) - (bv as number);
            return sortAsc ? cmp : -cmp;
        }),
    );

    function toggleSort(key: keyof SystemEntry) {
        if (sortKey === key) {
            sortAsc = !sortAsc;
        } else {
            sortKey = key;
            sortAsc = key === "name";
        }
    }
</script>

<div>
    <h2 class="section-title">System Hotspots</h2>
    <p class="section-subtitle">
        Ranked by total dirty hours · {weeks === 0
            ? "all time"
            : `${weeks} weeks`}
    </p>

    <table class="data-table">
        <thead>
            <tr>
                <th
                    class:is-sorted={sortKey === "name"}
                    onclick={() => toggleSort("name")}>System</th
                >
                <th
                    class="num"
                    class:is-sorted={sortKey === "sightings"}
                    onclick={() => toggleSort("sightings")}>Sightings</th
                >
                <th
                    class="num"
                    class:is-sorted={sortKey === "intervals"}
                    onclick={() => toggleSort("intervals")}>Intervals</th
                >
                <th
                    class="num"
                    class:is-sorted={sortKey === "dirty_hours"}
                    onclick={() => toggleSort("dirty_hours")}>Dirty Hours</th
                >
            </tr>
        </thead>
        <tbody>
            {#each sorted as sys}
                <tr>
                    <td class="name">{sys.name}</td>
                    <td class="num">{sys.sightings}</td>
                    <td class="num">{sys.intervals}</td>
                    <td class="num">{sys.dirty_hours.toFixed(1)}</td>
                </tr>
            {/each}
        </tbody>
    </table>
</div>
