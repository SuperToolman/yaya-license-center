"use client";
import {
  Accordion,
  Alert,
  Button,
  ButtonGroup,
  Card,
  Chip,
  Input,
  Label,
  Modal,
  Table,
  TextField,
} from "@heroui/react";
import { useState } from "react";
import { useOperationData as useData } from "../../../hooks/useOperationData";
import { useOperationMutation } from "../../../hooks/useOperationMutation";
import { formatDate } from "../../../lib/format";
import { operationCommand as operationJson } from "../../../lib/operation-api";
import type { ApplicationReleaseSubmission } from "../../../lib/operation-types";

export default function ApplicationSubmissionsModal({
  onClose,
  onReviewed,
  customerNames,
  customersLoading,
  customersError,
}: {
  onClose: () => void;
  onReviewed?: () => void;
  customerNames: Map<string, string>;
  customersLoading: boolean;
  customersError: string;
}) {
  const {
    data: result,
    error: queryError,
    loading,
    refresh,
  } = useData<ApplicationReleaseSubmission[]>("/api/application-submissions");
  const data = result ?? [];
  const mutation = useOperationMutation();
  const error = mutation.error || queryError;
  const [rejectingId, setRejectingId] = useState<string | null>(null);
  const [reason, setReason] = useState("");
  async function review(id: string, approved: boolean) {
    if (!approved && !reason.trim()) return;
    await mutation.run(
      () =>
        operationJson(
          `/api/application-submissions/${encodeURIComponent(id)}/review`,
          { approved, reason: approved ? "" : reason.trim() },
        ),
      () => {
        setRejectingId(null);
        setReason("");
        refresh();
        onReviewed?.();
      },
    );
  }
  const pendingCount = data.filter(
    (item) => item.status === "pending_review",
  ).length;
  const approvedCount = data.filter(
    (item) => item.status === "approved",
  ).length;
  const rejectedCount = data.filter(
    (item) => item.status === "rejected",
  ).length;
  return (
    <Modal isOpen onOpenChange={(open) => !open && onClose()}>
      <Modal.Backdrop>
        <Modal.Container
          placement="center"
          scroll="inside"
          className="w-full sm:w-full"
        >
          <Modal.Dialog className="max-w-[1680px]">
            <Modal.Header>
              <Modal.Heading>应用上线申请管理</Modal.Heading>
              <Modal.CloseTrigger aria-label="关闭" />
            </Modal.Header>
            <Modal.Body>
              <Alert>
                <Alert.Indicator />
                <Alert.Content>
                  <Alert.Description>
                    审核整包应用范围、售价与发布者分成；审核通过不会自动上架。
                  </Alert.Description>
                </Alert.Content>
              </Alert>
              {error ? (
                <Alert status="danger" role="alert">
                  <Alert.Indicator />
                  <Alert.Content>
                    <Alert.Title>{error}</Alert.Title>
                  </Alert.Content>
                </Alert>
              ) : null}
              {customersError ? (
                <Alert status="warning" role="alert">
                  <Alert.Indicator />
                  <Alert.Content>
                    <Alert.Title>
                      客户名称加载失败，暂时仅显示客户 ID。
                    </Alert.Title>
                  </Alert.Content>
                </Alert>
              ) : null}
              {loading ? <p role="status">正在加载应用申请…</p> : null}
              {rejectingId ? (
                <form
                  onSubmit={(event) => {
                    event.preventDefault();
                    void review(rejectingId, false);
                  }}
                >
                  <Card variant="secondary">
                    <Card.Header>
                      <Card.Title>
                        驳回申请：
                        {
                          data.find((item) => item.submissionId === rejectingId)
                            ?.appName
                        }
                      </Card.Title>
                    </Card.Header>
                    <Card.Content>
                      <TextField
                        name="reason"
                        isRequired
                        isDisabled={mutation.pending}
                        value={reason}
                        onChange={setReason}
                      >
                        <Label>驳回原因</Label>
                        <Input placeholder="请填写驳回原因" />
                      </TextField>
                    </Card.Content>
                    <Card.Footer>
                      <ButtonGroup>
                        <Button
                          type="submit"
                          variant="danger-soft"
                          isDisabled={!reason.trim()}
                          isPending={mutation.pending}
                        >
                          确认驳回
                        </Button>
                        <Button
                          variant="secondary"
                          isDisabled={mutation.pending}
                          onPress={() => {
                            setRejectingId(null);
                            setReason("");
                            mutation.clearError();
                          }}
                        >
                          取消驳回
                        </Button>
                      </ButtonGroup>
                    </Card.Footer>
                  </Card>
                </form>
              ) : null}
              <Table>
                <Table.ScrollContainer>
                  <Table.Content
                    aria-label="应用上线申请"
                    className="min-w-[1820px] table-fixed"
                  >
                    <Table.Header>
                      <Table.Column
                        isRowHeader
                        id="application"
                        className="w-[240px]"
                      >
                        应用
                      </Table.Column>
                      <Table.Column id="submissionType" className="w-[140px]">申请类型</Table.Column>
                      <Table.Column id="version" className="w-[120px]">
                        版本
                      </Table.Column>
                      <Table.Column id="pricing" className="w-[240px]">
                        费用
                      </Table.Column>
                      <Table.Column id="type" className="w-[340px]">
                        应用类型
                      </Table.Column>
                      <Table.Column id="customer" className="w-[440px]">
                        申请客户&amp;提交人
                      </Table.Column>
                      <Table.Column id="time" className="w-[180px]">
                        提交时间
                      </Table.Column>
                      <Table.Column id="status" className="w-[100px]">
                        状态
                      </Table.Column>
                      <Table.Column id="actions" className="w-[160px]">
                        操作
                      </Table.Column>
                    </Table.Header>
                    <Table.Body
                      renderEmptyState={() => (
                        <p>
                          {loading ? "正在加载应用申请…" : "暂无应用上线申请。"}
                        </p>
                      )}
                    >
                      {data.map((item) => {
                        const commercial = item.snapshot.commercial;
                        const manifest = item.snapshot.manifest;
                        const isSuite = manifest?.applicationType === "suite";
                        return (
                          <Table.Row
                            key={item.submissionId}
                            id={item.submissionId}
                          >
                            <Table.Cell className="align-middle break-words">
                              <strong>{item.appName}</strong>
                              {item.reviewReason ? (
                                <p className="text-muted">
                                  审核意见：{item.reviewReason}
                                </p>
                              ) : null}
                            </Table.Cell>
                            <Table.Cell className="align-middle"><Chip size="sm" variant="soft" color={item.snapshot.submissionType === "version_upgrade" ? "accent" : "default"}><Chip.Label>{item.snapshot.submissionType === "version_upgrade" ? "版本升级" : "新上线"}</Chip.Label></Chip><p className="mt-1 text-xs text-muted">{item.snapshot.source === "offline-import" ? "ZIP 导入" : "在线申请"}</p></Table.Cell>
                            <Table.Cell className="align-middle break-words">
                              v{item.version}
                            </Table.Cell>
                            <Table.Cell className="align-middle">
                              {commercial ? (
                                <>
                                  <p>
                                    {commercial.pricingMode === "free"
                                      ? "免费"
                                      : `¥${(commercial.priceCents / 100).toFixed(2)}`}
                                  </p>
                                  {commercial.publisherRateBasisPoints > 0 ? (
                                    <p className="text-xs text-muted">
                                      发布者分成{" "}
                                      {(
                                        commercial.publisherRateBasisPoints /
                                        100
                                      ).toFixed(2)}
                                      %
                                    </p>
                                  ) : null}
                                </>
                              ) : (
                                <p className="text-muted">
                                  缺少商业资料：请在商品销售配置中补齐
                                </p>
                              )}
                            </Table.Cell>
                            <Table.Cell className="align-middle">
                              <Accordion>
                                <Accordion.Item>
                                  <Accordion.Heading>
                                    <Accordion.Trigger
                                      className="gap-2"
                                      aria-label={`${isSuite ? "套应用" : "单应用"}：展开成员与依赖`}
                                    >
                                      <Chip
                                        size="sm"
                                        variant="primary"
                                        color="accent"
                                      >
                                        {isSuite ? "套应用" : "单应用"}
                                      </Chip>
                                      {isSuite ? (
                                        <span className="text-xs text-muted">
                                          {manifest?.members.length ?? 0} 个成员
                                        </span>
                                      ) : null}
                                      <Accordion.Indicator />
                                    </Accordion.Trigger>
                                  </Accordion.Heading>
                                  <Accordion.Panel>
                                    <Accordion.Body>
                                      <div className="break-all">
                                        {manifest?.members.map((member) => (
                                          <p key={member.appId}>
                                            {member.resources.apps[0]?.name ||
                                              member.appId}
                                            （{member.appId}）
                                          </p>
                                        ))}
                                        {manifest?.dependencies.map(
                                          (dependency, index) => (
                                            <p key={index}>
                                              {dependency.sourceAppId} →{" "}
                                              {dependency.targetAppId}：
                                              {dependency.path}
                                            </p>
                                          ),
                                        )}
                                      </div>
                                    </Accordion.Body>
                                  </Accordion.Panel>
                                </Accordion.Item>
                              </Accordion>
                            </Table.Cell>
                            <Table.Cell className="align-middle break-all">
                              <p>
                                <span>
                                  {customerNames.get(item.applicantSubject) ||
                                    (customersLoading
                                      ? "正在加载客户…"
                                      : customersError
                                        ? "客户名称暂不可用"
                                        : "未关联客户")}
                                </span>
                                {item.submittedBy ? (
                                  <span className="text-muted">
                                    {" "}
                                    ·{" "}
                                    {/^运营导入(?:[:：]|$)/.test(
                                      item.submittedBy,
                                    )
                                      ? "运营导入"
                                      : item.submittedBy}
                                  </span>
                                ) : null}
                              </p>
                              <p className="text-xs text-muted">
                                {item.applicantSubject || "客户 ID 缺失"}
                              </p>
                            </Table.Cell>
                            <Table.Cell className="align-middle whitespace-nowrap">
                              <time
                                dateTime={
                                  item.submittedAt
                                    ? new Date(
                                        item.submittedAt * 1000,
                                      ).toISOString()
                                    : undefined
                                }
                              >
                                {formatDate(item.submittedAt)}
                              </time>
                            </Table.Cell>
                            <Table.Cell className="align-middle">
                              <Chip
                                size="sm"
                                variant="soft"
                                color={
                                  item.status === "approved"
                                    ? "success"
                                    : item.status === "rejected"
                                      ? "danger"
                                      : "warning"
                                }
                              >
                                {item.status === "approved"
                                  ? "已通过"
                                  : item.status === "rejected"
                                    ? "已驳回"
                                    : item.status === "pending_review"
                                      ? "待审核"
                                      : item.status}
                              </Chip>
                            </Table.Cell>
                            <Table.Cell className="align-middle">
                              {item.status === "pending_review" ? (
                                <ButtonGroup size="sm">
                                  <Button
                                    isDisabled={mutation.pending}
                                    onPress={() =>
                                      void review(item.submissionId, true)
                                    }
                                  >
                                    同意
                                  </Button>
                                  <Button
                                    isDisabled={mutation.pending}
                                    variant="danger-soft"
                                    onPress={() => {
                                      setRejectingId(item.submissionId);
                                      setReason("");
                                      mutation.clearError();
                                    }}
                                  >
                                    驳回
                                  </Button>
                                </ButtonGroup>
                              ) : (
                                "—"
                              )}
                            </Table.Cell>
                          </Table.Row>
                        );
                      })}
                    </Table.Body>
                  </Table.Content>
                </Table.ScrollContainer>
                <Table.Footer>
                  共 {data.length} 条申请 · 待审核 {pendingCount} · 已通过{" "}
                  {approvedCount} · 已驳回 {rejectedCount}
                </Table.Footer>
              </Table>
            </Modal.Body>
            <Modal.Footer>
              <Button
                variant="secondary"
                size="sm"
                isDisabled={loading || mutation.pending}
                onPress={refresh}
              >
                刷新申请
              </Button>
            </Modal.Footer>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </Modal>
  );
}
