pipeline {
    agent {
        dockerfile {
            filename 'Dockerfile'
            pull true
        }
    }
    stages {
        stage('S') {
            steps { echo 'x' }
        }
    }
}
