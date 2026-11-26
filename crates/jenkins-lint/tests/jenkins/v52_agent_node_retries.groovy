pipeline {
    agent {
        node {
            label 'linux'
            retries 2
        }
    }
    stages {
        stage('S') {
            steps { echo 'x' }
        }
    }
}
