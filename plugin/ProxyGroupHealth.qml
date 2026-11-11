pragma ComponentBehavior: Bound
import QtQuick
import qs.Commons
import "." as Mihomo
import "ProxyModels.js" as Models

// Group form → Health check: the test URL, interval, tolerance and the
// result of a check right now.
Column {
  id: section
  // The group form: its draft, members, nodes and actions.
  required property var form
  width: parent.width
  spacing: Style.space(12)
  Txt {
    text: MihomoI18n.t("proxyHealthUrl")
  }
  Txt {
    text: MihomoI18n.t("proxyHealthUrlHint")
    dim: true
    font.pixelSize: Style.font.bodySmall
  }
  Seg {
    model: [
      {
        key: "https://www.gstatic.com/generate_204",
        label: "Google"
      },
      {
        key: "https://cp.cloudflare.com/generate_204",
        label: "Cloudflare"
      },
      {
        key: "custom",
        label: MihomoI18n.t("proxyCustom")
      }
    ]
    current: section.form.customUrl ? "custom" : !section.form.draft.url ? "https://www.gstatic.com/generate_204" : ["https://www.gstatic.com/generate_204", "https://cp.cloudflare.com/generate_204"].indexOf(section.form.draft.url) >= 0 ? section.form.draft.url : "custom"
    onPicked: function (k) {
      section.form.customUrl = k === "custom";
      section.form.editor.setField("url", k === "custom" ? "" : k);
    }
  }
  Field {
    width: parent.width
    text: section.form.draft.url === undefined ? "https://www.gstatic.com/generate_204" : section.form.draft.url
    placeholder: "https://www.gstatic.com/generate_204"
    input.onTextEdited: section.form.editor.setField("url", text)
  }
  Repeater {
    model: [
      {
        key: "interval",
        label: "proxyInterval",
        unit: "proxySeconds",
        value: 300
      },
      {
        key: "timeout",
        label: "proxyTimeout",
        unit: "ms",
        value: 5000
      }
    ]
    Row {
      id: numberRow
      required property var modelData
      width: parent.width
      spacing: Style.space(10)
      Txt {
        width: parent.width - Style.space(140)
        text: MihomoI18n.t(numberRow.modelData.label)
        anchors.verticalCenter: parent.verticalCenter
      }
      Field {
        width: Style.space(80)
        text: String(section.form.draft[numberRow.modelData.key] === undefined ? numberRow.modelData.value : section.form.draft[numberRow.modelData.key])
        input.validator: IntValidator {
          bottom: 1
          top: 2147483647
        }
        input.onTextEdited: {
          section.form.editor.issues = Models.put(section.form.editor.issues, numberRow.modelData.key, input.acceptableInput ? "" : MihomoI18n.t("proxyFixFields"));
          if (input.acceptableInput)
            section.form.editor.setField(numberRow.modelData.key, Number(text));
        }
      }
      Txt {
        text: MihomoI18n.t(numberRow.modelData.unit)
        dim: true
        anchors.verticalCenter: parent.verticalCenter
      }
    }
  }
  Row {
    width: parent.width
    Column {
      width: parent.width - lazySwitch.width
      spacing: Style.space(4)
      Txt {
        width: parent.width
        text: MihomoI18n.t("proxyLazy")
        wrapMode: Text.Wrap
      }
      Txt {
        width: parent.width
        text: MihomoI18n.t("proxyLazyHint")
        wrapMode: Text.Wrap
        dim: true
        font.pixelSize: Style.font.bodySmall
      }
    }
    Mihomo.Switch {
      id: lazySwitch
      checked: section.form.draft.lazy !== false
      onToggled: section.form.editor.setField("lazy", !checked)
    }
  }
  Btn {
    ghost: true
    text: MihomoI18n.t("proxySection_advanced")
    icon: section.form.healthAdvanced ? Icons.expanded : Icons.collapsed
    onClicked: section.form.healthAdvanced = !section.form.healthAdvanced
  }
  Repeater {
    model: section.form.editor.schema.fields.filter(function (f) {
      return ["expected-status", "empty-fallback", "max-failed-times"].indexOf(f.key) >= 0;
    })
    SchemaField {
      required property var modelData
      visible: section.form.healthAdvanced
      width: parent.width
      field: modelData
      value: section.form.draft[field.key]
      context: section.form.draft
      targets: section.form.editor.metadata.targets || []
      targetGroups: section.form.editor.metadata.groups || []
      targetDetails: section.form.editor.targetDetails
      onEdited: function (v) {
        section.form.editor.setField(field.key, v);
      }
      onIssue: function (e) {
        section.form.editor.issues = Models.put(section.form.editor.issues, field.key, e);
      }
    }
  }
  Row {
    width: parent.width
    Column {
      width: parent.width - checkButton.width
      Txt {
        text: MihomoI18n.t("proxyLastCheck")
      }
      Txt {
        text: section.form.checkedAt || "—"
        dim: true
        font.pixelSize: Style.font.bodySmall
      }
    }
    Btn {
      id: checkButton
      text: MihomoI18n.t("proxyCheckNow")
      icon: Icons.test
      busy: section.form.testing
      onClicked: section.form.check()
    }
  }
  Repeater {
    model: section.form.resultNames
    Row {
      required property string modelData
      width: parent.width
      Txt {
        width: parent.width - Style.space(100)
        text: parent.modelData
        elide: Text.ElideRight
      }
      Txt {
        text: section.form.label(parent.modelData)
        dim: true
      }
    }
  }
}
