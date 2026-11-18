node {
    stage('Build') {
        sh 'make || error "failed"'
    }
}
