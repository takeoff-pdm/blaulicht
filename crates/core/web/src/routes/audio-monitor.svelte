<script lang="ts">
	import { AreaChart } from 'layerchart';
	import TrendingUpIcon from '@lucide/svelte/icons/trending-up';
	import { curveNatural } from 'd3-shape';
	import { scaleUtc } from 'd3-scale';
	import * as Chart from '$lib/components/ui/chart/index.js';
	import * as Card from '$lib/components/ui/card/index.js';
	import { writable } from 'svelte/store';

	const { value = 0, className = '' } = $props<{ value: number; className: string }>();

	type Inner = { date: Date; value: number };
	// let chartData: Inner[] = $state([]);
	const chartData = writable([]);

	$effect(() => {
		// const v_value = value(); // or use `$value` if using auto-subscribed syntax
		if (value) {
			chartData.update((data: any[]) => {
				const newData = [...data, { date: new Date(), value: value }];
				return newData.slice(-100); // keep only last 100
			});
		}
	});

	// $effect(() => {
	// 	if (value) {
	// 		while (chartData.length > 100) {
	// 			chartData.shift();
	// 		}
	// 		chartData = [...chartData, { date: new Date(), value }];
	// 	}
	// });

	const chartConfig = {
		value: { label: 'Value', color: 'var(--chart-1)' }
	} satisfies Chart.ChartConfig;
</script>

<Card.Root class={className}>
	<Card.Header>
		<Card.Title>Area Chart</Card.Title>
		<Card.Description>Showing total visitors for the last 6 months</Card.Description>
	</Card.Header>
	<Card.Content>
		<Chart.Container config={chartConfig}>
			<AreaChart
				data={$chartData}
				x="date"
				xScale={scaleUtc()}
				series={[
					{
						key: 'value',
						label: 'Value',
						color: chartConfig.value.color
					}
				]}
				axis="x"
				props={{
					area: {
						curve: curveNatural,
						'fill-opacity': 0.4,
						line: { class: 'stroke-1' },
						motion: 'tween'
					},
					xAxis: {
						format: (v: Date) => v.getTime().toString()
					}
				}}
			>
				{#snippet tooltip()}
					<Chart.Tooltip
						labelFormatter={(v: Date) => v.toLocaleDateString('en-US', { month: 'long' })}
						indicator="line"
					/>
				{/snippet}
			</AreaChart>
		</Chart.Container>
	</Card.Content>
	<Card.Footer>
		<div class="flex w-full items-start gap-2 text-sm">
			<div class="grid gap-2">
				<div class="flex items-center gap-2 font-medium leading-none">
					Trending up by 5.2% this month <TrendingUpIcon class="size-4" />
				</div>
				<div class="text-muted-foreground flex items-center gap-2 leading-none">
					January - June 2024
				</div>
			</div>
		</div>
	</Card.Footer>
</Card.Root>
