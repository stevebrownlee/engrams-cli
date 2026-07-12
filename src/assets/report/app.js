document.addEventListener('DOMContentLoaded', () => {
  initHeader();
  initTabs();
  initOverview();
  initTables();
  initGraph();
});

// Helper: safe text formatting
function safeText(str) {
  return str === null || str === undefined ? '' : String(str);
}

// Helper: formatDate
function formatDate(ts) {
  if (!ts) return '-';
  return ts.replace('T', ' ').replace('Z', '').split('.')[0];
}

// 1. Header Information
function initHeader() {
  document.getElementById('header-db-path').textContent = ENGRAMS_DATA.db_path;
  document.getElementById('header-gen-time').textContent = formatDate(ENGRAMS_DATA.generated_at);
  document.getElementById('header-version').textContent = ENGRAMS_DATA.version;
}

// 2. Tab Switching
function initTabs() {
  const buttons = document.querySelectorAll('.tab-btn');
  const contents = document.querySelectorAll('.tab-content');

  buttons.forEach(btn => {
    btn.addEventListener('click', () => {
      const tabId = btn.getAttribute('data-tab');
      
      buttons.forEach(b => b.classList.remove('active'));
      contents.forEach(c => c.classList.remove('active'));
      
      btn.classList.add('active');
      const targetContent = document.getElementById(tabId);
      if (targetContent) {
        targetContent.classList.add('active');
      }

      // Re-run layout if Graph tab is selected and cytoscape is initialized
      if (tabId === 'tab-graph' && window.cyInstance) {
        window.cyInstance.resize();
        window.cyInstance.fit(undefined, 30);
      }
    });
  });
}

// 3. Overview Tab
function initOverview() {
  document.getElementById('card-decisions').textContent = ENGRAMS_DATA.decisions.length;
  document.getElementById('card-progress').textContent = ENGRAMS_DATA.progress.length;
  document.getElementById('card-patterns').textContent = ENGRAMS_DATA.patterns.length;
  document.getElementById('card-custom').textContent = ENGRAMS_DATA.custom_data.length;
  document.getElementById('card-links').textContent = ENGRAMS_DATA.links.length;

  renderContextPanel('doc-product-context', ENGRAMS_DATA.product_context);
  renderContextPanel('doc-active-context', ENGRAMS_DATA.active_context);
}

function renderContextPanel(elementId, contextDoc) {
  const container = document.getElementById(elementId);
  container.innerHTML = '';
  
  if (!contextDoc || !contextDoc.content) {
    const p = document.createElement('p');
    p.className = 'empty-table-message';
    p.textContent = '*(not set)*';
    container.appendChild(p);
    return;
  }

  const meta = document.createElement('p');
  meta.style.fontSize = '0.8rem';
  meta.style.color = '#868e96';
  meta.style.marginBottom = '0.5rem';
  meta.textContent = `Version: ${contextDoc.version} | Updated: ${formatDate(contextDoc.updated_at)}`;
  container.appendChild(meta);

  const content = contextDoc.content;
  if (typeof content === 'string') {
    const pre = document.createElement('pre');
    pre.textContent = content;
    container.appendChild(pre);
  } else if (typeof content === 'object') {
    const pre = document.createElement('pre');
    pre.textContent = JSON.stringify(content, null, 2);
    container.appendChild(pre);
  } else {
    const pre = document.createElement('pre');
    pre.textContent = String(content);
    container.appendChild(pre);
  }
}

// Helper: build details expandable box
function createDetailsRow(summaryText, fullObject) {
  const details = document.createElement('details');
  const summary = document.createElement('summary');
  summary.textContent = summaryText;
  details.appendChild(summary);

  const contentBox = document.createElement('div');
  contentBox.className = 'details-content-box';
  const pre = document.createElement('pre');
  pre.style.fontSize = '0.8rem';
  pre.style.overflowX = 'auto';
  pre.textContent = JSON.stringify(fullObject, null, 2);
  contentBox.appendChild(pre);
  details.appendChild(contentBox);
  
  return details;
}

// 4. Tables Population
function initTables() {
  // Decisions Table
  populateTable('table-decisions', ['ID', 'Summary', 'Rationale', 'Tags', 'Date'], ENGRAMS_DATA.decisions, (d) => {
    const tr = document.createElement('tr');
    
    const tdId = document.createElement('td');
    tdId.textContent = `#${d.id}`;
    tr.appendChild(tdId);

    const tdSummary = document.createElement('td');
    tdSummary.appendChild(createDetailsRow(d.summary, d));
    tr.appendChild(tdSummary);

    const tdRationale = document.createElement('td');
    tdRationale.textContent = safeText(d.rationale);
    tr.appendChild(tdRationale);

    const tdTags = document.createElement('td');
    if (d.tags && Array.isArray(d.tags)) {
      d.tags.forEach(t => {
        const badge = document.createElement('span');
        badge.className = 'tag-badge';
        badge.textContent = t;
        tdTags.appendChild(badge);
      });
    } else {
      tdTags.textContent = '-';
    }
    tr.appendChild(tdTags);

    const tdDate = document.createElement('td');
    tdDate.textContent = formatDate(d.timestamp);
    tr.appendChild(tdDate);

    return tr;
  });

  // Progress Table
  populateTable('table-progress', ['ID', 'Status', 'Description', 'Parent ID', 'Date'], ENGRAMS_DATA.progress, (p) => {
    const tr = document.createElement('tr');
    
    const tdId = document.createElement('td');
    tdId.textContent = `#${p.id}`;
    tr.appendChild(tdId);

    const tdStatus = document.createElement('td');
    const statusSpan = document.createElement('span');
    statusSpan.className = 'tag-badge';
    statusSpan.textContent = p.status;
    tdStatus.appendChild(statusSpan);
    tr.appendChild(tdStatus);

    const tdDesc = document.createElement('td');
    tdDesc.appendChild(createDetailsRow(p.description, p));
    tr.appendChild(tdDesc);

    const tdParent = document.createElement('td');
    tdParent.textContent = p.parent_id ? `#${p.parent_id}` : '-';
    tr.appendChild(tdParent);

    const tdDate = document.createElement('td');
    tdDate.textContent = formatDate(p.timestamp);
    tr.appendChild(tdDate);

    return tr;
  });

  // Patterns Table
  populateTable('table-patterns', ['ID', 'Name', 'Description', 'Tags', 'Date'], ENGRAMS_DATA.patterns, (pat) => {
    const tr = document.createElement('tr');
    
    const tdId = document.createElement('td');
    tdId.textContent = `#${pat.id}`;
    tr.appendChild(tdId);

    const tdName = document.createElement('td');
    tdName.appendChild(createDetailsRow(pat.name, pat));
    tr.appendChild(tdName);

    const tdDesc = document.createElement('td');
    tdDesc.textContent = safeText(pat.description);
    tr.appendChild(tdDesc);

    const tdTags = document.createElement('td');
    if (pat.tags && Array.isArray(pat.tags)) {
      pat.tags.forEach(t => {
        const badge = document.createElement('span');
        badge.className = 'tag-badge';
        badge.textContent = t;
        tdTags.appendChild(badge);
      });
    } else {
      tdTags.textContent = '-';
    }
    tr.appendChild(tdTags);

    const tdDate = document.createElement('td');
    tdDate.textContent = formatDate(pat.timestamp);
    tr.appendChild(tdDate);

    return tr;
  });

  // Custom Data Table
  populateTable('table-custom', ['ID', 'Category', 'Key', 'Value', 'Date'], ENGRAMS_DATA.custom_data, (c) => {
    const tr = document.createElement('tr');
    
    const tdId = document.createElement('td');
    tdId.textContent = `#${c.id}`;
    tr.appendChild(tdId);

    const tdCat = document.createElement('td');
    tdCat.textContent = c.category;
    tr.appendChild(tdCat);

    const tdKey = document.createElement('td');
    tdKey.appendChild(createDetailsRow(c.key, c));
    tr.appendChild(tdKey);

    const tdVal = document.createElement('td');
    tdVal.textContent = typeof c.value === 'object' ? JSON.stringify(c.value) : safeText(c.value);
    tr.appendChild(tdVal);

    const tdDate = document.createElement('td');
    tdDate.textContent = formatDate(c.timestamp);
    tr.appendChild(tdDate);

    return tr;
  });

  // Links Table
  populateTable('table-links', ['ID', 'Source', 'Target', 'Relationship', 'Description', 'Date'], ENGRAMS_DATA.links, (l) => {
    const tr = document.createElement('tr');
    
    const tdId = document.createElement('td');
    tdId.textContent = `#${l.id}`;
    tr.appendChild(tdId);

    const tdSrc = document.createElement('td');
    tdSrc.textContent = `${l.source_item_type} #${l.source_item_id}`;
    tr.appendChild(tdSrc);

    const tdTgt = document.createElement('td');
    tdTgt.textContent = `${l.target_item_type} #${l.target_item_id}`;
    tr.appendChild(tdTgt);

    const tdRel = document.createElement('td');
    tdRel.textContent = l.relationship_type;
    tr.appendChild(tdRel);

    const tdDesc = document.createElement('td');
    tdDesc.appendChild(createDetailsRow(l.description || '(none)', l));
    tr.appendChild(tdDesc);

    const tdDate = document.createElement('td');
    tdDate.textContent = formatDate(l.timestamp);
    tr.appendChild(tdDate);

    return tr;
  });
}

function populateTable(tableId, headers, data, rowGenerator) {
  const table = document.getElementById(tableId);
  table.innerHTML = '';

  if (!data || data.length === 0) {
    const tr = document.createElement('tr');
    const td = document.createElement('td');
    td.className = 'empty-table-message';
    td.setAttribute('colspan', headers.length);
    td.textContent = '(none recorded)';
    tr.appendChild(td);
    table.appendChild(tr);
    return;
  }

  // Header row
  const thead = document.createElement('thead');
  const headerTr = document.createElement('tr');
  headers.forEach(h => {
    const th = document.createElement('th');
    th.textContent = h;
    headerTr.appendChild(th);
  });
  thead.appendChild(headerTr);
  table.appendChild(thead);

  // Body rows
  const tbody = document.createElement('tbody');
  data.forEach(item => {
    tbody.appendChild(rowGenerator(item));
  });
  table.appendChild(tbody);
}

// 5. Knowledge Graph
window.cyInstance = null;
let graphNodes = [];
let graphEdges = [];
let allRelationshipTypes = new Set();
let focusNodeId = null;
let isEgoFocus = false; // Double-clicked into neighborhood focus

function initGraph() {
  buildGraphData();
  populateRelationshipFilter();
  setupGraphEvents();
  renderGraph();
}

function buildGraphData() {
  graphNodes = [];
  graphEdges = [];
  allRelationshipTypes.clear();

  const nodeMap = new Map();

  // Helper to safely truncate labels
  const truncate = (str) => {
    if (!str) return '';
    return str.length > 48 ? str.substring(0, 45) + '…' : str;
  };

  // Populate nodeMap and graphNodes from assets
  ENGRAMS_DATA.decisions.forEach(d => {
    const id = `decision:${d.id}`;
    const node = {
      data: {
        id,
        label: truncate(d.summary),
        type: 'decision',
        color: '#4f8ef7',
        borderWidth: 1,
        borderColor: '#2f6ecf',
        raw: d
      }
    };
    nodeMap.set(id, node);
  });

  ENGRAMS_DATA.progress.forEach(p => {
    const id = `progress_entry:${p.id}`;
    const isDone = /^(d|done|c|complete)$/i.test(p.status);
    const node = {
      data: {
        id,
        label: truncate(p.description),
        type: 'progress_entry',
        color: '#34c98e',
        borderWidth: isDone ? 3 : 1,
        borderColor: isDone ? '#107e54' : '#23a06e',
        raw: p
      }
    };
    nodeMap.set(id, node);
  });

  ENGRAMS_DATA.patterns.forEach(pat => {
    const id = `system_pattern:${pat.id}`;
    const node = {
      data: {
        id,
        label: truncate(pat.name),
        type: 'system_pattern',
        color: '#b678f2',
        borderWidth: 1,
        borderColor: '#985cd6',
        raw: pat
      }
    };
    nodeMap.set(id, node);
  });

  ENGRAMS_DATA.custom_data.forEach(c => {
    const id = `custom_data:${c.id}`;
    const node = {
      data: {
        id,
        label: truncate(`${c.category}/${c.key}`),
        type: 'custom_data',
        color: '#f2a13c',
        borderWidth: 1,
        borderColor: '#d28122',
        raw: c
      }
    };
    nodeMap.set(id, node);
  });

  // Keep track of node degrees
  const nodeDegrees = new Map();
  const incrementDegree = (id) => {
    nodeDegrees.set(id, (nodeDegrees.get(id) || 0) + 1);
  };

  let danglingCount = 0;

  // Process explicit links
  ENGRAMS_DATA.links.forEach(l => {
    const sourceId = `${l.source_item_type}:${l.source_item_id}`;
    const targetId = `${l.target_item_type}:${l.target_item_id}`;
    
    if (nodeMap.has(sourceId) && nodeMap.has(targetId)) {
      graphEdges.push({
        data: {
          id: `link:${l.id}`,
          source: sourceId,
          target: targetId,
          label: l.relationship_type,
          style: 'solid',
          lineColor: '#868e96',
          raw: l
        }
      });
      incrementDegree(sourceId);
      incrementDegree(targetId);
      allRelationshipTypes.add(l.relationship_type);
    } else {
      danglingCount++;
    }
  });

  // Process implicit progress parent edges
  ENGRAMS_DATA.progress.forEach(p => {
    if (p.parent_id) {
      const sourceId = `progress_entry:${p.id}`;
      const targetId = `progress_entry:${p.parent_id}`;

      if (nodeMap.has(sourceId) && nodeMap.has(targetId)) {
        graphEdges.push({
          data: {
            id: `progress_parent:${p.id}`,
            source: sourceId,
            target: targetId,
            label: 'subtask of',
            style: 'dashed',
            lineColor: '#adb5bd',
            raw: { relationship_type: 'subtask of' }
          }
        });
        incrementDegree(sourceId);
        incrementDegree(targetId);
      }
    }
  });

  // Store degree on nodes
  for (const [id, node] of nodeMap.entries()) {
    node.data.degree = nodeDegrees.get(id) || 0;
    graphNodes.push(node);
  }

  // Handle Dangling Banner
  const banner = document.getElementById('dangling-banner');
  if (danglingCount > 0) {
    banner.innerHTML = `<span>⚠️ ${danglingCount} dangling link(s) reference deleted items.</span><button onclick="this.parentElement.classList.add('hidden')" style="background:none;border:none;color:inherit;cursor:pointer;font-weight:bold;">✕</button>`;
    banner.classList.remove('hidden');
  } else {
    banner.classList.add('hidden');
  }

  // Handle empty graph message
  const emptyMsg = document.getElementById('graph-empty-message');
  if (graphEdges.length === 0) {
    emptyMsg.classList.remove('hidden');
  } else {
    emptyMsg.classList.add('hidden');
  }
}

function populateRelationshipFilter() {
  const container = document.getElementById('filter-relationship-types');
  container.innerHTML = '';

  // Add the implicit "subtask of" first if there are progress subtasks
  const relations = ['subtask of', ...Array.from(allRelationshipTypes).sort()];
  
  relations.forEach(rel => {
    const label = document.createElement('label');
    label.className = 'checkbox-label';
    
    const input = document.createElement('input');
    input.type = 'checkbox';
    input.setAttribute('data-rel', rel);
    input.checked = true;
    input.addEventListener('change', updateGraphFilter);

    label.appendChild(input);
    label.appendChild(document.createTextNode(rel));
    container.appendChild(label);
  });
}

function updateGraphFilter() {
  renderGraph();
}

function getFilteredElements() {
  // Checkboxes
  const showIsolated = document.getElementById('chk-show-isolated').checked;
  
  const activeNodeTypes = new Set(
    Array.from(document.querySelectorAll('#filter-node-types input[type="checkbox"]:checked'))
      .map(input => input.getAttribute('data-type'))
  );

  const activeEdgeTypes = new Set(
    Array.from(document.querySelectorAll('#filter-relationship-types input[type="checkbox"]:checked'))
      .map(input => input.getAttribute('data-rel'))
  );

  // First filter edges
  const filteredEdges = graphEdges.filter(edge => {
    return activeEdgeTypes.has(edge.data.label);
  });

  // Calculate degrees with ONLY active/filtered edges
  const activeNodeDegrees = new Map();
  filteredEdges.forEach(edge => {
    activeNodeDegrees.set(edge.data.source, (activeNodeDegrees.get(edge.data.source) || 0) + 1);
    activeNodeDegrees.set(edge.data.target, (activeNodeDegrees.get(edge.data.target) || 0) + 1);
  });

  // Filter nodes
  let filteredNodes = graphNodes.filter(node => {
    if (!activeNodeTypes.has(node.data.type)) return false;
    
    const degree = activeNodeDegrees.get(node.data.id) || 0;
    if (!showIsolated && degree === 0) return false;
    
    return true;
  });

  // If in ego-focus mode, restrict visible elements to neighborhood of focusNodeId
  if (isEgoFocus && focusNodeId) {
    const depth = parseInt(document.getElementById('focus-depth').value) || 1;
    const reachable = getKHopNeighborhood(filteredNodes, filteredEdges, focusNodeId, depth);
    
    filteredNodes = filteredNodes.filter(n => reachable.has(n.data.id));
    // Keep edges only if both endpoints are reachable
    return {
      nodes: filteredNodes,
      edges: filteredEdges.filter(e => reachable.has(e.data.source) && reachable.has(e.data.target))
    };
  }

  // Otherwise, return normal filtered elements
  const nodeIds = new Set(filteredNodes.map(n => n.data.id));
  return {
    nodes: filteredNodes,
    // Keep edges only if both nodes are present
    edges: filteredEdges.filter(e => nodeIds.has(e.data.source) && nodeIds.has(e.data.target))
  };
}

function getKHopNeighborhood(nodes, edges, startId, k) {
  const visited = new Set([startId]);
  let currentLevel = new Set([startId]);

  // Build adjacency list
  const adj = new Map();
  edges.forEach(e => {
    const s = e.data.source;
    const t = e.data.target;
    if (!adj.has(s)) adj.set(s, []);
    if (!adj.has(t)) adj.set(t, []);
    adj.get(s).push(t);
    adj.get(t).push(s);
  });

  for (let i = 0; i < k; i++) {
    const nextLevel = new Set();
    currentLevel.forEach(nodeId => {
      const neighbors = adj.get(nodeId) || [];
      neighbors.forEach(n => {
        if (!visited.has(n)) {
          visited.add(n);
          nextLevel.add(n);
        }
      });
    });
    if (nextLevel.size === 0) break;
    currentLevel = nextLevel;
  }

  return visited;
}

function renderGraph() {
  const container = document.getElementById('cy');
  if (!container) return;

  const elements = getFilteredElements();

  if (window.cyInstance) {
    window.cyInstance.destroy();
  }

  window.cyInstance = cytoscape({
    container: container,
    elements: [...elements.nodes, ...elements.edges],
    style: [
      {
        selector: 'node',
        style: {
          'label': 'data(label)',
          'background-color': 'data(color)',
          'border-width': 'data(borderWidth)',
          'border-color': 'data(borderColor)',
          'width': '40px',
          'height': '40px',
          'font-size': '10px',
          'text-valign': 'bottom',
          'text-margin-y': '5px',
          'text-wrap': 'wrap',
          'text-max-width': '100px',
          'color': '#212529',
          'font-weight': '500'
        }
      },
      {
        selector: 'edge',
        style: {
          'width': 1.5,
          'line-color': 'data(lineColor)',
          'line-style': 'data(style)',
          'target-arrow-color': 'data(lineColor)',
          'target-arrow-shape': 'triangle',
          'curve-style': 'bezier',
          // Show edge label only on hover via classes
          'label': '',
          'font-size': '8px',
          'color': '#495057',
          'text-background-opacity': 0.7,
          'text-background-color': '#ffffff',
          'text-background-padding': '2px',
          'text-background-shape': 'roundrectangle'
        }
      },
      {
        selector: 'edge.hover',
        style: {
          'label': 'data(label)'
        }
      },
      {
        selector: '.dimmed',
        style: {
          'opacity': 0.15
        }
      },
      {
        selector: '.highlighted-node',
        style: {
          'border-width': '3px',
          'border-color': '#000000',
          'width': '46px',
          'height': '46px'
        }
      },
      {
        selector: '.highlighted-edge',
        style: {
          'width': 3,
          'line-color': '#000000',
          'target-arrow-color': '#000000'
        }
      }
    ],
    layout: {
      name: 'cose',
      animate: false,
      fit: true,
      padding: 30
    }
  });

  // Restore focus visual if selected
  if (focusNodeId) {
    const node = window.cyInstance.getElementById(focusNodeId);
    if (node.length > 0) {
      highlightNodeInGraph(focusNodeId);
    }
  }

  // Handle Search on layout refresh
  applySearch();

  // Event handlers inside Cytoscape
  window.cyInstance.on('tap', 'node', (evt) => {
    const node = evt.target;
    focusNodeId = node.id();
    showNodeDetail(node.data());
    highlightNodeInGraph(focusNodeId);
  });

  window.cyInstance.on('dbltap', 'node', (evt) => {
    const node = evt.target;
    focusNodeId = node.id();
    isEgoFocus = true;
    showNodeDetail(node.data());
    renderGraph(); // Re-renders restricted to k-hop neighborhood
  });

  window.cyInstance.on('tap', (evt) => {
    if (evt.target === window.cyInstance) {
      clearGraphHighlights();
    }
  });

  // Edge hover effects
  window.cyInstance.on('mouseover', 'edge', (evt) => {
    evt.target.addClass('hover');
  });

  window.cyInstance.on('mouseout', 'edge', (evt) => {
    evt.target.removeClass('hover');
  });
}

function highlightNodeInGraph(nodeId) {
  if (!window.cyInstance) return;
  
  window.cyInstance.elements().removeClass('dimmed').removeClass('highlighted-node').removeClass('highlighted-edge');

  const selectedNode = window.cyInstance.getElementById(nodeId);
  if (selectedNode.length === 0) return;

  selectedNode.addClass('highlighted-node');

  const neighborhood = selectedNode.neighborhood();
  neighborhood.nodes().addClass('highlighted-node');
  neighborhood.edges().addClass('highlighted-edge');

  // Dim the rest
  const keepSet = new Set([nodeId, ...neighborhood.map(el => el.id())]);
  window.cyInstance.elements().forEach(el => {
    if (!keepSet.has(el.id())) {
      el.addClass('dimmed');
    }
  });
}

function clearGraphHighlights() {
  focusNodeId = null;
  isEgoFocus = false;
  hideNodeDetail();
  if (window.cyInstance) {
    window.cyInstance.elements().removeClass('dimmed').removeClass('highlighted-node').removeClass('highlighted-edge');
  }
}

function showNodeDetail(nodeData) {
  const panel = document.getElementById('detail-panel');
  const placeholder = panel.querySelector('.detail-placeholder');
  const content = document.getElementById('detail-content');
  const typeBadge = document.getElementById('detail-type');
  const title = document.getElementById('detail-title');
  const body = document.getElementById('detail-body-container');
  const depthContainer = document.getElementById('detail-focus-depth-container');

  placeholder.classList.add('hidden');
  content.classList.remove('hidden');

  if (isEgoFocus) {
    depthContainer.classList.remove('hidden');
  } else {
    depthContainer.classList.add('hidden');
  }

  // Type badge styling
  typeBadge.textContent = nodeData.type.replace('_', ' ');
  typeBadge.style.backgroundColor = nodeData.color;
  typeBadge.style.color = '#ffffff';

  // Title
  title.textContent = nodeData.label;

  // Body content dynamically generated
  body.innerHTML = '';
  const raw = nodeData.raw;

  const addField = (label, value) => {
    if (value === null || value === undefined) return;
    const field = document.createElement('div');
    field.className = 'detail-field';
    const flabel = document.createElement('span');
    flabel.className = 'detail-field-label';
    flabel.textContent = label;
    field.appendChild(flabel);

    const fval = document.createElement('div');
    fval.className = 'detail-field-value';
    if (typeof value === 'object') {
      const pre = document.createElement('pre');
      pre.style.fontSize = '0.75rem';
      pre.style.background = '#f8f9fa';
      pre.style.padding = '0.5rem';
      pre.style.borderRadius = '4px';
      pre.style.border = '1px solid #e9ecef';
      pre.textContent = JSON.stringify(value, null, 2);
      fval.appendChild(pre);
    } else {
      fval.textContent = String(value);
    }
    field.appendChild(fval);
    body.appendChild(field);
  };

  // Add type-specific fields
  addField('ID', raw.id);
  if (nodeData.type === 'decision') {
    addField('UUID', raw.uuid);
    addField('Summary', raw.summary);
    addField('Rationale', raw.rationale);
    addField('Implementation Details', raw.implementation_details);
    addField('Tags', raw.tags);
  } else if (nodeData.type === 'progress_entry') {
    addField('Status', raw.status);
    addField('Description', raw.description);
    addField('Parent ID', raw.parent_id);
  } else if (nodeData.type === 'system_pattern') {
    addField('UUID', raw.uuid);
    addField('Name', raw.name);
    addField('Description', raw.description);
    addField('Tags', raw.tags);
  } else if (nodeData.type === 'custom_data') {
    addField('Category', raw.category);
    addField('Key', raw.key);
    addField('Value', raw.value);
  }
  addField('Timestamp', formatDate(raw.timestamp));

  // Connections list in details
  const linksList = document.getElementById('detail-links-list');
  linksList.innerHTML = '';

  const incoming = [];
  const outgoing = [];

  graphEdges.forEach(edge => {
    if (edge.data.source === nodeData.id) {
      outgoing.push(edge);
    } else if (edge.data.target === nodeData.id) {
      incoming.push(edge);
    }
  });

  const renderLinkItem = (edge, isIncoming) => {
    const li = document.createElement('li');
    const roleSpan = document.createElement('span');
    roleSpan.style.fontWeight = 'bold';
    roleSpan.style.color = '#868e96';
    roleSpan.style.marginRight = '0.4rem';
    roleSpan.textContent = isIncoming ? '←' : '→';
    li.appendChild(roleSpan);

    const relSpan = document.createElement('span');
    relSpan.style.fontStyle = 'italic';
    relSpan.textContent = `${edge.data.label} `;
    li.appendChild(relSpan);

    const otherId = isIncoming ? edge.data.source : edge.data.target;
    const parts = otherId.split(':');
    const type = parts[0];
    const idVal = parts[1];

    const link = document.createElement('a');
    link.textContent = `${type.replace('_', ' ')} #${idVal}`;
    link.addEventListener('click', () => {
      // Find node in cy
      if (window.cyInstance) {
        const cyNode = window.cyInstance.getElementById(otherId);
        if (cyNode.length > 0) {
          window.cyInstance.center(cyNode);
          focusNodeId = otherId;
          const nodeDataObj = cyNode.data();
          showNodeDetail(nodeDataObj);
          highlightNodeInGraph(focusNodeId);
        }
      }
    });

    li.appendChild(link);
    linksList.appendChild(li);
  };

  incoming.forEach(edge => renderLinkItem(edge, true));
  outgoing.forEach(edge => renderLinkItem(edge, false));

  if (incoming.length === 0 && outgoing.length === 0) {
    const li = document.createElement('li');
    li.style.color = '#adb5bd';
    li.style.fontStyle = 'italic';
    li.textContent = 'No connections';
    linksList.appendChild(li);
  }
}

function hideNodeDetail() {
  const panel = document.getElementById('detail-panel');
  const placeholder = panel.querySelector('.detail-placeholder');
  const content = document.getElementById('detail-content');
  
  placeholder.classList.remove('hidden');
  content.classList.add('hidden');
}

function setupGraphEvents() {
  // Show isolated items checkbox
  document.getElementById('chk-show-isolated').addEventListener('change', updateGraphFilter);

  // Item type filters
  const nodeCheckboxes = document.querySelectorAll('#filter-node-types input[type="checkbox"]');
  nodeCheckboxes.forEach(cb => {
    cb.addEventListener('change', updateGraphFilter);
  });

  // Re-layout button
  document.getElementById('btn-re-layout').addEventListener('click', () => {
    if (window.cyInstance) {
      window.cyInstance.layout({ name: 'cose', animate: false, fit: true, padding: 30 }).run();
    }
  });

  // Reset view button
  document.getElementById('btn-reset-view').addEventListener('click', () => {
    clearGraphHighlights();
    renderGraph();
  });

  // Search input
  document.getElementById('search-input').addEventListener('input', applySearch);

  // Focus depth select
  document.getElementById('focus-depth').addEventListener('change', () => {
    if (isEgoFocus) {
      renderGraph();
    }
  });

  // Keyboard Escape listener
  document.addEventListener('keydown', (evt) => {
    if (evt.key === 'Escape') {
      clearGraphHighlights();
      renderGraph();
    }
  });
}

function applySearch() {
  if (!window.cyInstance) return;

  const query = document.getElementById('search-input').value.trim().toLowerCase();
  
  if (!query) {
    // If no query, make sure to clear opacity overrides
    window.cyInstance.elements().removeClass('search-matched').removeClass('search-dimmed');
    return;
  }

  window.cyInstance.nodes().forEach(node => {
    const data = node.data();
    const label = data.label.toLowerCase();
    
    // Build search text from object fields
    let fullText = label;
    if (data.raw) {
      const raw = data.raw;
      if (raw.description) fullText += ' ' + raw.description.toLowerCase();
      if (raw.rationale) fullText += ' ' + raw.rationale.toLowerCase();
      if (raw.implementation_details) fullText += ' ' + raw.implementation_details.toLowerCase();
      if (raw.category) fullText += ' ' + raw.category.toLowerCase();
      if (raw.key) fullText += ' ' + raw.key.toLowerCase();
      if (raw.tags && Array.isArray(raw.tags)) {
        fullText += ' ' + raw.tags.join(' ').toLowerCase();
      }
    }

    if (fullText.includes(query)) {
      node.removeClass('dimmed');
    } else {
      node.addClass('dimmed');
    }
  });
}
