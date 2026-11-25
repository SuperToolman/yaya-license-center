"use client";
import { Button } from "@heroui/react";
import { ModalShell } from "../../../components/ui/ModalShell";
import { useOperationData } from "../../../hooks/useOperationData";
import { useOperationMutation } from "../../../hooks/useOperationMutation";
import { operationCommand } from "../../../lib/operation-api";
type Ledger = { orders: { orderId:string; subject:string; productId:string; status:string; amountCents:number; snapshot:{name:string}; refundReason?:string }[]; revenues:{id:string; publisherSubject:string; orderId:string; amountCents:number; status:string}[]; settlements:{id:string;publisherSubject:string;amountCents:number;status:string}[] };
const money=(v:number)=>`¥${(v/100).toFixed(2)}`;
export default function ApplicationCommerceModal({onClose}:{onClose:()=>void}){
 const {data,refresh,error}=useOperationData<Ledger>("/api/application-commerce");const mutation=useOperationMutation();
 async function refund(orderId:string,approved:boolean){const reason=approved?"":window.prompt("拒绝退款原因");if(!approved&&!reason?.trim())return;await mutation.run(()=>operationCommand(`/api/application-commerce/${encodeURIComponent(orderId)}/refund`,{approved,reason}),refresh);}
 async function settle(publisherSubject:string){await mutation.run(()=>operationCommand("/api/application-commerce/settlements",{publisherSubject}),refresh);}
 return <ModalShell title="应用交易与分成" width={1100} onClose={onClose}><p className="mb-5 rounded-xl bg-accent-soft p-4 text-sm">以下全部为模拟交易台账。退款与结算不发生实际资金流转；权益由交易自动发放，单应用和套应用均按整个商品处理。</p>{(error||mutation.error)&&<p role="alert">{error||mutation.error}</p>}
 <h3 className="my-3 font-semibold">订单与退款</h3><div className="space-y-3">{data?.orders.map(o=><article key={o.orderId} className="rounded-xl border p-4"><div className="flex flex-wrap justify-between gap-3"><strong>{o.snapshot.name} · {money(o.amountCents)}</strong><span>{o.status}</span></div><p className="my-2 text-xs text-muted">购买客户：{o.subject} · {o.orderId}</p>{o.refundReason&&<p>退款原因：{o.refundReason}</p>}{o.status==="refund_requested"&&<div className="flex gap-2"><Button size="sm" isDisabled={mutation.pending} onPress={()=>void refund(o.orderId,true)}>同意模拟退款</Button><Button size="sm" variant="secondary" isDisabled={mutation.pending} onPress={()=>void refund(o.orderId,false)}>拒绝退款</Button></div>}</article>)}</div>
 <h3 className="my-5 font-semibold">发布者收益</h3>{[...new Set(data?.revenues.map(r=>r.publisherSubject))].map(subject=>{const entries=data?.revenues.filter(r=>r.publisherSubject===subject)??[];const pending=entries.filter(r=>r.status==="pending_settlement").reduce((a,r)=>a+r.amountCents,0);return <article key={subject} className="my-3 rounded-xl border p-4"><div className="flex flex-wrap items-center justify-between gap-3"><strong>{subject} · 待结算 {money(pending)}</strong><Button size="sm" isDisabled={mutation.pending||pending<=0} onPress={()=>void settle(subject)}>登记模拟结算</Button></div>{entries.map(r=><p key={r.id} className="mt-2 text-xs text-muted">{r.orderId} · {money(r.amountCents)} · {r.status}</p>)}</article>;})}
 <h3 className="my-5 font-semibold">结算批次</h3>{data?.settlements.map(s=><p key={s.id} className="my-2 text-sm">{s.publisherSubject} · {money(s.amountCents)} · 模拟结算已登记 · {s.id}</p>)}{data?.orders.length===0&&<p>暂无应用订单。</p>}</ModalShell>;
}
