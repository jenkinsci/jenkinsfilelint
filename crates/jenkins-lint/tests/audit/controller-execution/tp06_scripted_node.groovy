node('master') {
    checkout scm
    sh 'make'
}

node(label: 'built-in') {
    sh 'make publish'
}

node 'built-in', {
    sh 'make clean'
}
