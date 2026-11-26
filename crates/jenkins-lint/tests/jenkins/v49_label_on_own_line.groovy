def find(items) {
    outer:
    for (a in items) {
        for (b in a) {
            if (b) {
                break outer
            }
        }
    }
}

pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                script {
                    def hit = false
                    search:
                    for (x in [1, 2]) {
                        if (x == 2) {
                            hit = true
                            break search
                        }
                    }
                    if (hit) {
                        again:
                        while (true) {
                            break again
                        }
                    }
                }
            }
        }
    }
}
