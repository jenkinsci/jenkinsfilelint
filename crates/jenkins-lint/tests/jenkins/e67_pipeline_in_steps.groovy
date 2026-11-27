pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                pipeline {
                    agent any
                }
            }
        }
    }
}
