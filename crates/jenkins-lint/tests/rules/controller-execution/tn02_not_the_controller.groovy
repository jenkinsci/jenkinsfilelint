// The controller is left out, or the label only looks like its name.
node('!master') {
    sh 'make'
}

node('linux && !built-in') {
    sh 'make'
}

node('build-master') {
    sh 'make'
}

node('masters') {
    sh 'make'
}
