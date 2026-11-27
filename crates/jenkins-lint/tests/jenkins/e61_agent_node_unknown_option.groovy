pipeline {
    agent {
        node {
            label 'linux'
            pull true
        }
    }
    stages {
        stage('S') {
            steps { echo 'x' }
        }
    }
}
