pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                if (params.X) {
                    echo 'y'
                }
            }
        }
    }
}
